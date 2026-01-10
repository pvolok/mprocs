use std::{ffi::CString, os::fd::OwnedFd, ptr::null};

use rustix::{fs::OFlags, process::WaitStatus, pty::OpenptFlags, termios::Pid};
use tokio::io::unix::AsyncFd;

use crate::{
  process::{process::Process, unix_processes_waiter::UnixProcessesWaiter},
  term_types::winsize::Winsize,
};

use super::process_spec::ProcessSpec;

pub struct UnixProcess {
  pub pid: Pid,
  master: AsyncFd<OwnedFd>,
}

impl UnixProcess {
  pub fn spawn(
    spec: &ProcessSpec,
    size: Winsize,
    on_wait_returned: Box<dyn Fn(WaitStatus) + Send + Sync>,
  ) -> std::io::Result<Self> {
    unsafe {
      let master = {
        let mut flags = OpenptFlags::RDWR | OpenptFlags::NOCTTY;
        // TODO: Only add on macos.
        flags |= OpenptFlags::from_bits_retain(libc::O_NONBLOCK as u32);
        rustix::pty::openpt(flags)?
      };

      rustix::pty::grantpt(&master)?;
      rustix::pty::unlockpt(&master)?;

      // TODO: This is needed on linux/bsd.
      // let mut flags = rustix::io::fcntl_getfd(&master)?;
      // flags |= FdFlags::from_bits_retain(libc::O_NONBLOCK as u32);
      // rustix::io::fcntl_setfd(&master, flags)?;

      let slave_name = rustix::pty::ptsname(&master, Vec::new())?;

      let (sync_r, sync_w) = rustix::pipe::pipe()?;

      let pid = libc::fork();
      if pid < 0 {
        return Err(std::io::Error::last_os_error());
      }

      if pid == 0 {
        drop(master);
        for signo in &[
          libc::SIGCHLD,
          libc::SIGHUP,
          libc::SIGINT,
          libc::SIGQUIT,
          libc::SIGTERM,
          libc::SIGALRM,
        ] {
          libc::signal(*signo, libc::SIG_DFL);
        }

        let slave_fd = rustix::fs::open(
          slave_name,
          OFlags::RDWR | OFlags::NOCTTY,
          rustix::fs::Mode::empty(),
        )
        .unwrap();

        rustix::stdio::dup2_stdin(&slave_fd).unwrap();
        rustix::stdio::dup2_stdout(&slave_fd).unwrap();
        rustix::stdio::dup2_stderr(&slave_fd).unwrap();
        drop(slave_fd);

        if let Some(cwd) = spec.get_cwd() {
          rustix::process::chdir(cwd).unwrap();
        }

        for (key, value) in &spec.env {
          if let Some(value) = value.as_ref() {
            std::env::set_var(key, value);
          } else {
            std::env::remove_var(key);
          }
        }

        rustix::process::setsid().unwrap();
        libc::ioctl(0, libc::TIOCSCTTY as _, 0);

        rustix::io::write(&sync_w, b"1")?;
        drop(sync_r);
        drop(sync_w);

        let prog = CString::new(spec.prog.as_str()).unwrap_or_default();
        let mut argv = Vec::new();
        argv.push(prog.clone());
        for arg in &spec.args {
          argv.push(CString::new(arg.as_str()).unwrap_or_default());
        }
        let argv_ptrs = argv.iter().map(|a| a.as_ptr()).collect::<Vec<_>>();
        libc::execvp(prog.as_ptr(), argv_ptrs.as_ptr());
        libc::perror(null());
        libc::_exit(1);
      }
      let pid = Pid::from_raw_unchecked(pid);

      drop(sync_w);
      rustix::io::read(&sync_r, &mut [0])?;
      drop(sync_r);

      rustix::termios::tcsetwinsize(&master, size.into())?;

      UnixProcessesWaiter::wait_for(pid, on_wait_returned);

      Ok(UnixProcess {
        pid,
        master: AsyncFd::new(master)?,
      })
    }
  }
}

impl Process for UnixProcess {
  async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    loop {
      let mut guard = self.master.readable().await?;
      match guard.try_io(|fd| Ok(rustix::io::read(fd, &mut *buf)?)) {
        Ok(result) => {
          break Ok(result?);
        }
        Err(_would_block) => {
          continue;
        }
      }
    }
  }

  async fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    loop {
      let mut guard = self.master.writable().await?;
      match guard.try_io(|fd| Ok(rustix::io::write(fd, buf)?)) {
        Ok(result) => {
          break Ok(result?);
        }
        Err(_would_block) => {
          continue;
        }
      }
    }
  }

  async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
    let mut count = 0;
    while count < buf.len() {
      count += self.write(&buf[count..]).await?;
    }
    Ok(())
  }

  fn send_signal(&mut self, sig: i32) -> std::io::Result<()> {
    if unsafe { libc::kill(self.pid.as_raw_nonzero().into(), sig) } < 0 {
      return Err(std::io::Error::last_os_error());
    }
    Ok(())
  }

  async fn kill(&mut self) -> std::io::Result<()> {
    self.send_signal(libc::SIGKILL)
  }

  fn resize(&mut self, size: Winsize) -> std::io::Result<()> {
    rustix::termios::tcsetwinsize(&self.master, size.into())?;
    Ok(())
  }
}
