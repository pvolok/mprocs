use crossterm::event::Event;
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::{io::Write, time::Duration};

use crate::{
  error::ResultLogger,
  term::{
    input_parser::InputParser,
    internal::{InternalTermEvent, KeyboardMode},
  },
};

pub struct TermDriver {
  #[cfg(unix)]
  stdin: rustix::fd::BorrowedFd<'static>,
  #[cfg(unix)]
  orig_termios: rustix::termios::Termios,
  #[cfg(unix)]
  exit_write: std::os::unix::net::UnixStream,

  #[cfg(windows)]
  win_vt: super::windows::WinVt,

  stdout: std::io::Stdout,

  events: tokio::sync::mpsc::UnboundedReceiver<InternalTermEvent>,

  init_timeout: Option<tokio::task::JoinHandle<()>>,
  keyboard: KeyboardMode,
}

#[cfg(unix)]
const WAKE_BYTE_QUIT: u8 = b'q';

impl TermDriver {
  pub fn create() -> anyhow::Result<Self> {
    #[cfg(unix)]
    let stdin = rustix::stdio::stdin();
    #[cfg(unix)]
    if !rustix::termios::isatty(stdin) {
      anyhow::bail!("Stdin is not a tty.");
    }

    #[cfg(windows)]
    let win_vt = super::windows::WinVt::enable()?;

    let mut stdout = std::io::stdout();

    let (sender, events) = tokio::sync::mpsc::unbounded_channel();

    #[cfg(unix)]
    let orig_termios = rustix::termios::tcgetattr(stdin)?;
    #[cfg(unix)]
    let mut termios = orig_termios.clone();
    #[cfg(unix)]
    termios.make_raw();
    #[cfg(unix)]
    rustix::termios::tcsetattr(
      stdin,
      rustix::termios::OptionalActions::Now,
      &termios,
    )?;

    // Save Cursor (DECSC)
    stdout.write_all(b"\x1b7")?;
    // Enter alternate screen.
    stdout.write_all(b"\x1B[?1049h")?;
    // Clear all.
    stdout.write_all(b"\x1B[2J")?;
    // Mouse
    {
      // Normal tracking: Send mouse X & Y on button press and release
      stdout.write_all(b"\x1B[?1000h")?;
      // Button-event tracking: Report button motion events (dragging)
      stdout.write_all(b"\x1B[?1002h")?;
      // Any-event tracking: Report all motion events
      stdout.write_all(b"\x1B[?1003h")?;
      // RXVT mouse mode: Allows mouse coordinates of >223
      stdout.write_all(b"\x1B[?1015h")?;
      // SGR mouse mode: Allows mouse coordinates of >223, preferred over RXVT mode
      stdout.write_all(b"\x1B[?1006h")?;
    }

    // Query kitty keyboard protocol.
    stdout.write_all(b"\x1B[?u")?;
    // Query device.
    stdout.write_all(b"\x1B[c")?;

    let init_timeout = {
      let sender = sender.clone();
      tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        sender.send(InternalTermEvent::InitTimeout).log_ignore();
      })
    };

    #[cfg(unix)]
    let (exit_read, exit_write) =
      std::os::unix::net::UnixStream::pair().unwrap();
    #[cfg(unix)]
    {
      exit_read.set_nonblocking(true).unwrap();
      exit_write.set_nonblocking(true).unwrap();
      std::thread::spawn(move || {
        let (mut sig_read, sig_write) =
          std::os::unix::net::UnixStream::pair().unwrap();
        sig_read.set_nonblocking(true).unwrap();
        sig_write.set_nonblocking(true).unwrap();
        signal_hook::low_level::pipe::register(
          signal_hook::consts::SIGWINCH,
          sig_write,
        )
        .unwrap();

        let mut read_buf = unsafe { Box::new_uninit_slice(1024).assume_init() };
        let mut input_parser = InputParser::new();

        'thread: loop {
          let fds = [
            stdin.as_raw_fd(),
            sig_read.as_raw_fd(),
            exit_read.as_raw_fd(),
          ];
          let nfds = fds.iter().copied().max().unwrap_or(0) + 1;
          let mut fdset =
            vec![
              rustix::event::FdSetElement::default();
              rustix::event::fd_set_num_elements(fds.len(), nfds)
            ];

          for fd in fds {
            rustix::event::fd_set_insert(&mut fdset, fd);
          }
          unsafe {
            rustix::event::select(nfds, Some(&mut fdset), None, None, None)
              .unwrap()
          };

          for fd in rustix::event::FdSetIter::new(&fdset) {
            if fd == stdin.as_raw_fd() {
              match rustix::io::read(stdin, Box::as_mut(&mut read_buf)) {
                Ok(read_count) => {
                  let slice = &read_buf[..read_count];
                  input_parser.parse_input(slice, true, false, |e| {
                    sender.send(e).log_ignore()
                  });
                }
                Err(err) => log::error!("stdin(err): {:?}", err),
              }
            } else if fd == sig_read.as_raw_fd() {
              std::io::Read::read_exact(&mut sig_read, &mut [0]).unwrap();
              let winsize = rustix::termios::tcgetwinsize(stdin).unwrap();
              sender
                .send(InternalTermEvent::Resize(winsize.ws_col, winsize.ws_row))
                .log_ignore();
            } else if fd == exit_read.as_raw_fd() {
              break 'thread;
            }
          }
        }
      });
    }

    #[cfg(windows)]
    unsafe {
      std::thread::spawn(move || {
        let stdin = winapi::um::processenv::GetStdHandle(
          winapi::um::winbase::STD_INPUT_HANDLE,
        );
        let mut input_parser = InputParser::new();
        let mut buf = [winapi::um::wincon::INPUT_RECORD::default(); 128];
        loop {
          let mut count = 0;
          winapi::um::consoleapi::ReadConsoleInputA(
            stdin,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut count,
          );

          super::windows::decode_input_records(
            &mut input_parser,
            &buf[..count as usize],
            &mut |event| {
              let _ = sender.send(event);
            },
          );
        }
      });
    };

    Ok(Self {
      #[cfg(unix)]
      stdin,
      #[cfg(unix)]
      orig_termios,
      #[cfg(unix)]
      exit_write,

      #[cfg(windows)]
      win_vt,

      stdout,

      events,

      init_timeout: Some(init_timeout),
      keyboard: KeyboardMode::Unknown,
    })
  }

  pub fn destroy(&mut self) -> anyhow::Result<()> {
    match self.keyboard {
      KeyboardMode::Unknown => (),
      KeyboardMode::ModifyOtherKeys => {
        self.stdout.write_all(b"\x1b[>4;0m")?;
      }
      KeyboardMode::Kitty => {
        self.stdout.write_all(b"\x1b[<1u")?;
      }
      KeyboardMode::Win32 => {
        self.stdout.write_all(b"\x1b[?9001l")?;
      }
    }

    // Mouse
    {
      self.stdout.write_all(b"\x1B[?1006l")?;
      self.stdout.write_all(b"\x1B[?1015l")?;
      self.stdout.write_all(b"\x1B[?1003l")?;
      self.stdout.write_all(b"\x1B[?1002l")?;
      self.stdout.write_all(b"\x1B[?1000l")?;
    }
    // Leave alternate screen.
    self.stdout.write_all(b"\x1B[?1049l")?;

    // Save/Restore does not work on tmux. So we just show cursor.
    self.stdout.write_all(b"\x1b[?25h")?;
    // Restore Cursor (DECRC)
    self.stdout.write_all(b"\x1b8")?;

    #[cfg(unix)]
    rustix::termios::tcsetattr(
      self.stdin,
      rustix::termios::OptionalActions::Now,
      &self.orig_termios,
    )?;

    #[cfg(unix)]
    self.exit_write.write_all(&[WAKE_BYTE_QUIT])?;

    #[cfg(windows)]
    self.win_vt.disable();

    Ok(())
  }

  pub async fn input(&mut self) -> std::io::Result<Option<Event>> {
    loop {
      let event = if let Some(event) = self.events.recv().await {
        event
      } else {
        return Ok(None);
      };
      match event {
        InternalTermEvent::Key(key_event) => {
          return Ok(Some(Event::Key(key_event)));
        }
        InternalTermEvent::Mouse(mouse_event) => {
          return Ok(Some(Event::Mouse(mouse_event)));
        }
        InternalTermEvent::Resize(cols, rows) => {
          return Ok(Some(Event::Resize(cols, rows)))
        }
        InternalTermEvent::FocusGained => return Ok(Some(Event::FocusGained)),
        InternalTermEvent::FocusLost => return Ok(Some(Event::FocusLost)),
        InternalTermEvent::CursorPos(_x, _y) => (),
        InternalTermEvent::PrimaryDeviceAttributes => {
          if let Some(timeout) = &self.init_timeout {
            timeout.abort();
          }
          self.init_timeout = None;
          if matches!(self.keyboard, KeyboardMode::Unknown) {
            #[cfg(unix)]
            {
              self.keyboard = KeyboardMode::ModifyOtherKeys;
              self.stdout.write_all(b"\x1b[>4;2m")?;
            }
            #[cfg(windows)]
            {
              self.keyboard = KeyboardMode::Win32;
              self.stdout.write_all(b"\x1b[?9001h")?;
            }
          }
        }

        InternalTermEvent::InitTimeout => {
          self.init_timeout = None;
          if matches!(self.keyboard, KeyboardMode::Unknown) {
            #[cfg(unix)]
            {
              self.keyboard = KeyboardMode::ModifyOtherKeys;
              self.stdout.write_all(b"\x1b[>4;2m")?;
            }
            #[cfg(windows)]
            {
              self.keyboard = KeyboardMode::Win32;
              self.stdout.write_all(b"\x1b[?9001h")?;
            }
          }
        }
        InternalTermEvent::ReplyKittyKeyboard(_flags) => {
          self.keyboard = KeyboardMode::Kitty;
          // 0b1 (1) - Disambiguate escape codes
          // 0b10 (2) - Report event types
          // 0b100 (4) - Report alternate keys
          // 0b1000 (8) - Report all keys as escape codes
          // 0b10000 (16) - Report associated text
          // 0b1111 = 15
          self.stdout.write_all(b"\x1b[>15u")?;
        }
      };
    }
  }
}
