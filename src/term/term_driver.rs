use std::{
  io::{Read, Write},
  os::{fd::AsRawFd, unix::net::UnixStream},
  time::Duration,
};

use anyhow::bail;
use crossterm::event::Event;
use rustix::termios::isatty;
use signal_hook::consts::SIGWINCH;

use crate::{
  error::ResultLogger,
  term::{
    input_parser::InputParser,
    internal::{InternalTermEvent, KeyboardMode},
  },
};

pub struct TermDriver {
  stdin: rustix::fd::BorrowedFd<'static>,
  stdout: std::io::Stdout,
  orig_termios: rustix::termios::Termios,
  exit_write: UnixStream,

  events: tokio::sync::mpsc::UnboundedReceiver<InternalTermEvent>,

  init_timeout: Option<tokio::task::JoinHandle<()>>,
  keyboard: KeyboardMode,
}

const WAKE_BYTE_QUIT: u8 = b'q';

impl TermDriver {
  pub fn create() -> anyhow::Result<Self> {
    let stdin = rustix::stdio::stdin();
    let mut stdout = std::io::stdout();
    if !isatty(stdin) {
      bail!("Stdin is not a tty.");
    }

    let (sender, events) = tokio::sync::mpsc::unbounded_channel();

    let orig_termios = rustix::termios::tcgetattr(stdin)?;
    let mut termios = orig_termios.clone();
    termios.make_raw();
    rustix::termios::tcsetattr(
      stdin,
      rustix::termios::OptionalActions::Now,
      &termios,
    )?;
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

    let (exit_read, exit_write) = UnixStream::pair().unwrap();
    exit_read.set_nonblocking(true).unwrap();
    exit_write.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
      let (mut sig_read, sig_write) = UnixStream::pair().unwrap();
      sig_read.set_nonblocking(true).unwrap();
      sig_write.set_nonblocking(true).unwrap();
      signal_hook::low_level::pipe::register(SIGWINCH, sig_write).unwrap();

      let mut read_buf = unsafe { Box::new_uninit_slice(1024).assume_init() };
      let mut input_parser = InputParser::new();

      'thread: loop {
        let fds = [
          stdin.as_raw_fd(),
          sig_read.as_raw_fd(),
          exit_read.as_raw_fd(),
        ];
        let nfds = fds.iter().copied().max().unwrap_or(0) + 1;
        let mut fdset = vec![
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
                input_parser
                  .parse_input(slice, true, |e| sender.send(e).log_ignore());
              }
              Err(err) => log::error!("stdin(err): {:?}", err),
            }
          } else if fd == sig_read.as_raw_fd() {
            sig_read.read_exact(&mut [0]).unwrap();
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

    Ok(Self {
      stdin,
      stdout,
      orig_termios,
      exit_write,

      events,

      init_timeout: Some(init_timeout),
      keyboard: KeyboardMode::Unknown,
    })
  }

  pub fn destroy(mut self) -> anyhow::Result<()> {
    match self.keyboard {
      KeyboardMode::Unknown => (),
      KeyboardMode::ModifyOtherKeys => {
        self.stdout.write_all(b"\x1b[>4;0m")?;
      }
      KeyboardMode::Kitty(_) => {
        self.stdout.write_all(b"\x1b[<1u")?;
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

    rustix::termios::tcsetattr(
      self.stdin,
      rustix::termios::OptionalActions::Now,
      &self.orig_termios,
    )?;

    self.exit_write.write_all(&[WAKE_BYTE_QUIT])?;

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
          return Ok(Some(Event::Key(key_event)))
        }
        InternalTermEvent::Mouse(mouse_event) => {
          return Ok(Some(Event::Mouse(mouse_event)))
        }
        InternalTermEvent::Resize(cols, rows) => {
          return Ok(Some(Event::Resize(cols, rows)))
        }
        InternalTermEvent::InitTimeout => {
          if matches!(self.keyboard, KeyboardMode::Unknown) {
            self.keyboard = KeyboardMode::ModifyOtherKeys;
            self.stdout.write_all(b"\x1b[>4;2m")?;
          }
        }
        InternalTermEvent::ReplyKittyKeyboard(flags) => {
          self.keyboard = KeyboardMode::Kitty(flags);
          self.stdout.write_all(b"\x1b[>1u")?;
        }
      };
    }
  }
}
