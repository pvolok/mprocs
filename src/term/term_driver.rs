use std::{
  io::Read,
  os::{fd::AsRawFd, unix::net::UnixStream},
};

use anyhow::bail;
use crossterm::event::Event;
use rustix::termios::isatty;
use signal_hook::consts::SIGWINCH;

use crate::{error::ResultLogger, term::input_parser::InputParser};

pub struct TermDriver {
  stdin: rustix::fd::BorrowedFd<'static>,
  termios: rustix::termios::Termios,
}

impl TermDriver {
  pub fn create() -> anyhow::Result<Self> {
    let stdin = rustix::stdio::stdin();
    if !isatty(stdin) {
      bail!("Stdin is not a tty.");
    }

    let termios = rustix::termios::tcgetattr(stdin)?;
    Ok(Self { stdin, termios })
  }

  pub fn enable_tui(&mut self) -> anyhow::Result<()> {
    // let mut termios = self.termios.clone();
    // termios.make_raw();

    Ok(())
  }

  pub fn listen<
    E: Send + 'static,
    F: Fn(Event) -> E + Sync + Send + 'static,
  >(
    &mut self,
    f: F,
    sender: tokio::sync::mpsc::UnboundedSender<E>,
  ) -> anyhow::Result<()> {
    let stdin = self.stdin;
    std::thread::spawn(move || {
      let (mut sig_read, sig_write) = UnixStream::pair().unwrap();
      sig_read.set_nonblocking(true).unwrap();
      sig_write.set_nonblocking(true).unwrap();
      signal_hook::low_level::pipe::register(SIGWINCH, sig_write).unwrap();

      let mut read_buf = unsafe { Box::new_uninit_slice(1024).assume_init() };
      let mut input_parser = InputParser::new();

      let (wake_read, wake_write) = UnixStream::pair().unwrap();
      wake_read.set_nonblocking(true).unwrap();
      wake_write.set_nonblocking(true).unwrap();

      loop {
        let fds = [stdin.as_raw_fd(), sig_read.as_raw_fd()];
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
                  .parse_input(slice, |e| sender.send(f(e)).log_ignore());
              }
              Err(err) => log::error!("stdin(err): {:?}", err),
            }
          } else if fd == sig_read.as_raw_fd() {
            sig_read.read_exact(&mut [0]).unwrap();
            let winsize = rustix::termios::tcgetwinsize(stdin).unwrap();
            sender
              .send(f(crossterm::event::Event::Resize(
                winsize.ws_col,
                winsize.ws_row,
              )))
              .log_ignore();
          }
        }
      }
    });
    Ok(())
  }
}
