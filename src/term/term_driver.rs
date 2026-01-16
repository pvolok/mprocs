use crossterm::event::Event;
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd};
use std::{io::Write, time::Duration};
use termwiz::escape::{csi::Sgr, Action, OneBased, CSI};
use tui::style::Modifier;

use crate::{
  error::ResultLogger,
  protocol::Color,
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

impl tui::backend::Backend for TermDriver {
  fn draw<'a, I>(&mut self, content: I) -> std::io::Result<()>
  where
    I: Iterator<Item = (u16, u16, &'a tui::buffer::Cell)>,
  {
    let mut fg = tui::style::Color::Reset;
    let mut bg = tui::style::Color::Reset;
    let mut modifier = Modifier::empty();
    let mut last_pos: Option<tui::layout::Position> = None;
    let mut out = std::io::stdout();
    for (x, y, cell) in content {
      // Move the cursor if the previous location was not (x - 1, y)
      if !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y) {
        let action =
          Action::CSI(CSI::Cursor(termwiz::escape::csi::Cursor::Position {
            line: OneBased::from_zero_based(y.into()),
            col: OneBased::from_zero_based(x.into()),
          }));
        write!(out, "{}", action)?;
      }
      last_pos = Some(tui::layout::Position { x, y });
      if cell.modifier != modifier {
        let removed = modifier - cell.modifier;
        let added = cell.modifier - modifier;

        if removed.contains(Modifier::REVERSED) {
          let action = Action::CSI(CSI::Sgr(Sgr::Inverse(false)));
          write!(out, "{}", action)?;
        }
        if removed.contains(Modifier::BOLD) || removed.contains(Modifier::DIM) {
          // Bold and Dim are both reset by applying the Normal intensity
          let action = Action::CSI(CSI::Sgr(Sgr::Intensity(
            termwiz::cell::Intensity::Normal,
          )));
          write!(out, "{}", action)?;

          // The remaining Bold and Dim attributes must be
          // reapplied after the intensity reset above.
          if cell.modifier.contains(Modifier::DIM) {
            let action = Action::CSI(CSI::Sgr(Sgr::Intensity(
              termwiz::cell::Intensity::Half,
            )));
            write!(out, "{}", action)?;
          }

          if cell.modifier.contains(Modifier::BOLD) {
            let action = Action::CSI(CSI::Sgr(Sgr::Intensity(
              termwiz::cell::Intensity::Bold,
            )));
            write!(out, "{}", action)?;
          }
        }
        if removed.contains(Modifier::ITALIC) {
          let action = Action::CSI(CSI::Sgr(Sgr::Italic(false)));
          write!(out, "{}", action)?;
        }
        if removed.contains(Modifier::UNDERLINED) {
          let action = Action::CSI(CSI::Sgr(Sgr::Underline(
            termwiz::cell::Underline::None,
          )));
          write!(out, "{}", action)?;
        }
        if removed.contains(Modifier::CROSSED_OUT) {
          let action = Action::CSI(CSI::Sgr(Sgr::StrikeThrough(false)));
          write!(out, "{}", action)?;
        }
        if removed.contains(Modifier::SLOW_BLINK)
          || removed.contains(Modifier::RAPID_BLINK)
        {
          let action =
            Action::CSI(CSI::Sgr(Sgr::Blink(termwiz::cell::Blink::None)));
          write!(out, "{}", action)?;
        }

        if added.contains(Modifier::REVERSED) {
          let action = Action::CSI(CSI::Sgr(Sgr::Inverse(true)));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::BOLD) {
          let action = Action::CSI(CSI::Sgr(Sgr::Intensity(
            termwiz::cell::Intensity::Bold,
          )));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::ITALIC) {
          let action = Action::CSI(CSI::Sgr(Sgr::Italic(true)));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::UNDERLINED) {
          let action = Action::CSI(CSI::Sgr(Sgr::Underline(
            termwiz::cell::Underline::Single,
          )));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::DIM) {
          let action = Action::CSI(CSI::Sgr(Sgr::Intensity(
            termwiz::cell::Intensity::Half,
          )));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::CROSSED_OUT) {
          let action = Action::CSI(CSI::Sgr(Sgr::StrikeThrough(true)));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::SLOW_BLINK) {
          let action =
            Action::CSI(CSI::Sgr(Sgr::Blink(termwiz::cell::Blink::Slow)));
          write!(out, "{}", action)?;
        }
        if added.contains(Modifier::RAPID_BLINK) {
          let action =
            Action::CSI(CSI::Sgr(Sgr::Blink(termwiz::cell::Blink::Rapid)));
          write!(out, "{}", action)?;
        }

        modifier = cell.modifier;
      }
      if cell.fg != fg || cell.bg != bg {
        let action =
          Action::CSI(CSI::Sgr(Sgr::Foreground(Color::from(cell.fg).into())));
        write!(out, "{}", action)?;
        let action =
          Action::CSI(CSI::Sgr(Sgr::Background(Color::from(cell.bg).into())));
        write!(out, "{}", action)?;
        fg = cell.fg;
        bg = cell.bg;
      }

      write!(out, "{}", cell.symbol())?;
    }

    let action = Action::CSI(CSI::Sgr(Sgr::Foreground(
      termwiz::color::ColorSpec::Default,
    )));
    write!(out, "{}", action)?;
    let action = Action::CSI(CSI::Sgr(Sgr::Background(
      termwiz::color::ColorSpec::Default,
    )));
    write!(out, "{}", action)?;

    Ok(())
  }

  fn hide_cursor(&mut self) -> std::io::Result<()> {
    let action =
      Action::CSI(CSI::Mode(termwiz::escape::csi::Mode::ResetDecPrivateMode(
        termwiz::escape::csi::DecPrivateMode::Code(
          termwiz::escape::csi::DecPrivateModeCode::ShowCursor,
        ),
      )));
    write!(std::io::stdout(), "{}", action)?;
    Ok(())
  }

  fn show_cursor(&mut self) -> std::io::Result<()> {
    let action =
      Action::CSI(CSI::Mode(termwiz::escape::csi::Mode::SetDecPrivateMode(
        termwiz::escape::csi::DecPrivateMode::Code(
          termwiz::escape::csi::DecPrivateModeCode::ShowCursor,
        ),
      )));
    write!(std::io::stdout(), "{}", action)?;
    Ok(())
  }

  fn get_cursor_position(&mut self) -> std::io::Result<tui::prelude::Position> {
    // Only called for Viewport::Inline
    log::error!("TermDriver::get_cursor_position() should not be called.");
    Ok(Default::default())
  }

  fn set_cursor_position<P: Into<tui::prelude::Position>>(
    &mut self,
    position: P,
  ) -> std::io::Result<()> {
    let pos = position.into();
    let action =
      Action::CSI(CSI::Cursor(termwiz::escape::csi::Cursor::Position {
        line: OneBased::from_zero_based(pos.y.into()),
        col: OneBased::from_zero_based(pos.x.into()),
      }));
    write!(std::io::stdout(), "{}", action)?;
    Ok(())
  }

  fn clear(&mut self) -> std::io::Result<()> {
    let action =
      Action::CSI(CSI::Edit(termwiz::escape::csi::Edit::EraseInDisplay(
        termwiz::escape::csi::EraseInDisplay::EraseDisplay,
      )));
    write!(std::io::stdout(), "{}", action)?;
    Ok(())
  }

  #[cfg(unix)]
  fn size(&self) -> std::io::Result<tui::prelude::Size> {
    let size = rustix::termios::tcgetwinsize(self.stdin.as_fd())?;
    Ok(tui::layout::Size {
      width: size.ws_col,
      height: size.ws_row,
    })
  }

  #[cfg(windows)]
  fn size(&self) -> std::io::Result<tui::prelude::Size> {
    use std::os::windows::io::AsRawHandle;

    let mut info: winapi::um::wincon::CONSOLE_SCREEN_BUFFER_INFO =
      unsafe { std::mem::zeroed() };
    unsafe {
      winapi::um::wincon::GetConsoleScreenBufferInfo(
        self.stdout.as_raw_handle(),
        &mut info,
      )
    };
    let x = info.srWindow.Right - info.srWindow.Left + 1;
    let y = info.srWindow.Bottom - info.srWindow.Top + 1;
    Ok(tui::layout::Size {
      width: x as u16,
      height: y as u16,
    })
  }

  #[cfg(unix)]
  fn window_size(&mut self) -> std::io::Result<tui::backend::WindowSize> {
    let size = rustix::termios::tcgetwinsize(self.stdin.as_fd())?;
    Ok(tui::backend::WindowSize {
      columns_rows: tui::layout::Size {
        width: size.ws_col,
        height: size.ws_row,
      },
      pixels: tui::layout::Size {
        width: size.ws_xpixel,
        height: size.ws_ypixel,
      },
    })
  }

  #[cfg(windows)]
  fn window_size(&mut self) -> std::io::Result<tui::backend::WindowSize> {
    use std::os::windows::io::AsRawHandle;

    let mut info: winapi::um::wincon::CONSOLE_SCREEN_BUFFER_INFO =
      unsafe { std::mem::zeroed() };
    unsafe {
      winapi::um::wincon::GetConsoleScreenBufferInfo(
        self.stdout.as_raw_handle(),
        &mut info,
      )
    };
    let x = info.srWindow.Right - info.srWindow.Left + 1;
    let y = info.srWindow.Bottom - info.srWindow.Top + 1;
    Ok(tui::backend::WindowSize {
      columns_rows: tui::layout::Size {
        width: x as u16,
        height: y as u16,
      },
      pixels: tui::layout::Size {
        width: 0,
        height: 0,
      },
    })
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.stdout.flush()
  }
}
