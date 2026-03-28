use std::collections::VecDeque;
use std::io::Write;
use std::pin::Pin;
use std::time::Duration;

mod input_parser;
pub(crate) mod internal;
#[cfg(windows)]
mod windows;

use crate::{error::ResultLogger, term::{TermEvent, Size}};

use self::{
  input_parser::InputParser,
  internal::{InternalTermEvent, KeyboardMode},
};

pub struct TermDriver {
  #[cfg(unix)]
  stdin_fd: rustix::fd::BorrowedFd<'static>,
  #[cfg(unix)]
  orig_termios: rustix::termios::Termios,
  #[cfg(unix)]
  stdin_thread: Option<std::thread::JoinHandle<()>>,
  #[cfg(unix)]
  stdin_wakeup: std::os::fd::OwnedFd,
  #[cfg(unix)]
  sigwinch: tokio::signal::unix::Signal,

  #[cfg(windows)]
  win_vt: windows::WinVt,

  events:
    tokio::sync::mpsc::UnboundedReceiver<std::io::Result<InternalTermEvent>>,

  stdout: std::io::Stdout,

  pending: VecDeque<InternalTermEvent>,
  init_timeout: Option<Pin<Box<tokio::time::Sleep>>>,
  keyboard: KeyboardMode,
}

impl TermDriver {
  pub fn create() -> anyhow::Result<Self> {
    #[cfg(unix)]
    let stdin_fd = rustix::stdio::stdin();
    #[cfg(unix)]
    if !rustix::termios::isatty(stdin_fd) {
      anyhow::bail!("Stdin is not a tty.");
    }

    #[cfg(windows)]
    let win_vt = windows::WinVt::enable()?;

    let mut stdout = std::io::stdout();

    #[cfg(unix)]
    let orig_termios = rustix::termios::tcgetattr(stdin_fd)?;
    #[cfg(unix)]
    let mut termios = orig_termios.clone();
    #[cfg(unix)]
    termios.make_raw();
    #[cfg(unix)]
    rustix::termios::tcsetattr(
      stdin_fd,
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

    #[cfg(unix)]
    let sigwinch = tokio::signal::unix::signal(
      tokio::signal::unix::SignalKind::window_change(),
    )
    .expect("Failed to register SIGWINCH handler");

    #[cfg(unix)]
    let (events, stdin_thread, stdin_wakeup) = {
      use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

      let (sender, events) = tokio::sync::mpsc::unbounded_channel();
      let mut pipe_fds = [0; 2];
      unsafe {
        if libc::pipe(pipe_fds.as_mut_ptr()) < 0 {
          return Err(std::io::Error::last_os_error().into());
        }
      }

      let wake_read = unsafe { OwnedFd::from_raw_fd(pipe_fds[0]) };
      let wake_write = unsafe { OwnedFd::from_raw_fd(pipe_fds[1]) };
      let stdin_raw = stdin_fd.as_raw_fd();

      let stdin_thread = std::thread::spawn(move || {
        let mut input_parser = InputParser::new();
        let mut read_buf = [0u8; 1024];

        loop {
          let mut poll_fds = [
            libc::pollfd {
              fd: stdin_raw,
              events: libc::POLLIN,
              revents: 0,
            },
            libc::pollfd {
              fd: wake_read.as_raw_fd(),
              events: libc::POLLIN,
              revents: 0,
            },
          ];

          // Note: tty stdin can only be awaited with select/poll on Macos.
          let poll_result = unsafe { libc::poll(poll_fds.as_mut_ptr(), 2, -1) };
          if poll_result < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
              continue;
            }
            let _ = sender.send(Err(err));
            break;
          }

          if (poll_fds[1].revents & libc::POLLIN) != 0 {
            break;
          }

          if (poll_fds[0].revents & (libc::POLLIN | libc::POLLHUP)) != 0 {
            let n = match rustix::io::read(stdin_fd, &mut read_buf) {
              Ok(n) => n,
              Err(err) => {
                let io_err = std::io::Error::from(err);
                if io_err.kind() == std::io::ErrorKind::Interrupted {
                  continue;
                }
                let _ = sender.send(Err(io_err));
                break;
              }
            };

            if n == 0 {
              break;
            }

            input_parser.parse_input(&read_buf[..n], true, false, |event| {
              let _ = sender.send(Ok(event));
            });
          }
        }
      });
      (events, Some(stdin_thread), wake_write)
    };

    #[cfg(windows)]
    let events = {
      let (sender, events) = tokio::sync::mpsc::unbounded_channel();
      unsafe {
        std::thread::spawn(move || {
          let stdin = match windows::Win32::System::Console::GetStdHandle(
            windows::Win32::System::Console::STD_INPUT_HANDLE,
          ) {
            Ok(stdin) => stdin,
            Err(err) => {
              log::error!("GetStdHandle error: {}", err);
              return;
            }
          };
          let mut input_parser = InputParser::new();
          let mut buf =
            [windows::Win32::System::Console::INPUT_RECORD::default(); 128];
          loop {
            let mut count = 0;
            match windows::Win32::System::Console::ReadConsoleInputA(
              stdin, &mut buf, &mut count,
            ) {
              Ok(()) => (),
              Err(err) => {
                log::error!("ReadConsoleInputA error: {}", err);
                break;
              }
            };

            windows::decode_input_records(
              &mut input_parser,
              &buf[..count as usize],
              &mut |event| {
                let _ = sender.send(Ok(event));
              },
            );
          }
        });
      };
      events
    };

    Ok(Self {
      #[cfg(unix)]
      stdin_fd,
      #[cfg(unix)]
      orig_termios,
      #[cfg(unix)]
      stdin_thread,
      #[cfg(unix)]
      stdin_wakeup,
      #[cfg(unix)]
      sigwinch,

      #[cfg(windows)]
      win_vt,

      events,

      stdout,
      pending: VecDeque::new(),
      init_timeout: Some(Box::pin(tokio::time::sleep(Duration::from_millis(
        200,
      )))),
      keyboard: KeyboardMode::Unknown,
    })
  }

  fn handle_internal(
    &mut self,
    event: InternalTermEvent,
  ) -> std::io::Result<Option<TermEvent>> {
    match event {
      InternalTermEvent::Key(key_event) => {
        return Ok(Some(TermEvent::Key(key_event)));
      }
      InternalTermEvent::Mouse(mouse_event) => {
        return Ok(Some(TermEvent::Mouse(mouse_event)));
      }
      InternalTermEvent::Resize(cols, rows) => {
        return Ok(Some(TermEvent::Resize(cols, rows)));
      }
      InternalTermEvent::FocusGained => {
        return Ok(Some(TermEvent::FocusGained));
      }
      InternalTermEvent::FocusLost => return Ok(Some(TermEvent::FocusLost)),
      InternalTermEvent::CursorPos(_x, _y) => (),
      InternalTermEvent::PrimaryDeviceAttributes => {
        self.init_timeout = None;
        self.activate_keyboard_fallback()?;
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
    Ok(None)
  }

  fn activate_keyboard_fallback(&mut self) -> std::io::Result<()> {
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
    Ok(())
  }

  #[cfg(unix)]
  pub async fn input(&mut self) -> std::io::Result<Option<TermEvent>> {
    loop {
      // Drain buffered events first.
      while let Some(event) = self.pending.pop_front() {
        if let Some(term_event) = self.handle_internal(event)? {
          return Ok(Some(term_event));
        }
      }

      tokio::select! {
        event = self.events.recv() => {
          match event {
            Some(Ok(event)) => {
              if let Some(term_event) = self.handle_internal(event)? {
                return Ok(Some(term_event));
              }
            }
            Some(Err(err)) => return Err(err),
            None => return Ok(None),
          }
        }
        _ = self.sigwinch.recv() => {
          let winsize = rustix::termios::tcgetwinsize(self.stdin_fd)?;
          return Ok(Some(TermEvent::Resize(winsize.ws_col, winsize.ws_row)));
        }
        _ = async {
          match &mut self.init_timeout {
            Some(sleep) => sleep.await,
            None => std::future::pending().await,
          }
        } => {
          self.init_timeout = None;
          self.activate_keyboard_fallback()?;
        }
      }
    }
  }

  #[cfg(windows)]
  pub async fn input(&mut self) -> std::io::Result<Option<TermEvent>> {
    loop {
      // Drain buffered events first.
      while let Some(event) = self.pending.pop_front() {
        if let Some(term_event) = self.handle_internal(event)? {
          return Ok(Some(term_event));
        }
      }

      tokio::select! {
        event = self.events.recv() => {
          match event {
            Some(Ok(event)) => {
              if let Some(term_event) = self.handle_internal(event)? {
                return Ok(Some(term_event));
              }
            }
            Some(Err(err)) => return Err(err),
            None => return Ok(None),
          }
        }
        _ = async {
          match &mut self.init_timeout {
            Some(sleep) => sleep.await,
            None => std::future::pending().await,
          }
        } => {
          self.init_timeout = None;
          self.activate_keyboard_fallback()?;
        }
      }
    }
  }

  #[cfg(unix)]
  pub fn size(&self) -> std::io::Result<Size> {
    let winsize = rustix::termios::tcgetwinsize(self.stdin_fd)?;
    Ok(Size {
      height: winsize.ws_row,
      width: winsize.ws_col,
    })
  }

  #[cfg(windows)]
  pub fn size(&self) -> std::io::Result<Size> {
    unsafe {
      use std::os::windows::io::AsRawHandle;

      use windows::Win32::{
        Foundation::HANDLE,
        System::Console::{
          GetConsoleScreenBufferInfo, CONSOLE_SCREEN_BUFFER_INFO,
        },
      };

      let mut csbi: CONSOLE_SCREEN_BUFFER_INFO = Default::default();

      GetConsoleScreenBufferInfo(
        HANDLE(self.stdout.as_raw_handle()),
        &mut csbi,
      )?;

      Ok(Size {
        height: (csbi.srWindow.Bottom - csbi.srWindow.Top + 1) as u16,
        width: (csbi.srWindow.Right - csbi.srWindow.Left + 1) as u16,
      })
    }
  }
}

impl Drop for TermDriver {
  fn drop(&mut self) {
    match self.keyboard {
      KeyboardMode::Unknown => (),
      KeyboardMode::ModifyOtherKeys => {
        self.stdout.write_all(b"\x1b[>4;0m").log_ignore();
      }
      KeyboardMode::Kitty => {
        self.stdout.write_all(b"\x1b[<1u").log_ignore();
      }
      KeyboardMode::Win32 => {
        self.stdout.write_all(b"\x1b[?9001l").log_ignore();
      }
    }

    // Mouse
    {
      self.stdout.write_all(b"\x1B[?1006l").log_ignore();
      self.stdout.write_all(b"\x1B[?1015l").log_ignore();
      self.stdout.write_all(b"\x1B[?1003l").log_ignore();
      self.stdout.write_all(b"\x1B[?1002l").log_ignore();
      self.stdout.write_all(b"\x1B[?1000l").log_ignore();
    }
    // Leave alternate screen.
    self.stdout.write_all(b"\x1B[?1049l").log_ignore();

    // Save/Restore does not work on tmux. So we just show cursor.
    self.stdout.write_all(b"\x1b[?25h").log_ignore();
    // Restore Cursor (DECRC)
    self.stdout.write_all(b"\x1b8").log_ignore();

    self.stdout.flush().log_ignore();

    #[cfg(unix)]
    rustix::io::write(&self.stdin_wakeup, &[0]).log_ignore();

    #[cfg(unix)]
    if let Some(stdin_thread) = self.stdin_thread.take() {
      stdin_thread.join().ok();
    }

    #[cfg(unix)]
    rustix::termios::tcsetattr(
      self.stdin_fd,
      rustix::termios::OptionalActions::Now,
      &self.orig_termios,
    )
    .log_ignore();

    #[cfg(windows)]
    self.win_vt.disable();
  }
}
