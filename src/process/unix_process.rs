use std::{
  ffi::CString,
  os::fd::{FromRawFd, OwnedFd},
  ptr::{null, null_mut},
};

use rustix::{process::WaitStatus, termios::Pid};
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
      let mut set = std::mem::zeroed();
      libc::sigfillset(&mut set);
      libc::sigprocmask(libc::SIG_SETMASK, &set, null_mut());

      let mut master = -1;
      // Some args are *mut on some BSD vaiants.
      #[allow(clippy::unnecessary_mut_passed)]
      let pid =
        libc::forkpty(&mut master, null_mut(), null_mut(), &mut size.into());
      if pid < 0 {
        return Err(std::io::Error::last_os_error());
      }

      if pid == 0 {
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
        libc::sigemptyset(&mut set);
        libc::sigprocmask(libc::SIG_SETMASK, &set, null_mut());

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

        let prog = CString::new(spec.prog.as_str()).unwrap_or_default();
        let mut argv = Vec::new();
        argv.push(prog.clone());
        for arg in &spec.args {
          argv.push(CString::new(arg.as_str()).unwrap_or_default());
        }
        let mut argv_ptrs = argv.iter().map(|a| a.as_ptr()).collect::<Vec<_>>();
        argv_ptrs.push(null());
        libc::execvp(prog.as_ptr(), argv_ptrs.as_ptr());
        libc::perror(null());
        libc::_exit(1);
      }

      libc::sigemptyset(&mut set);
      libc::sigprocmask(libc::SIG_SETMASK, &set, null_mut());

      let flags = libc::fcntl(master, libc::F_GETFD, 0);
      if flags < 0 {
        return Err(std::io::Error::last_os_error());
      }
      if libc::fcntl(master, libc::F_SETFD, flags | libc::FD_CLOEXEC) < 0 {
        return Err(std::io::Error::last_os_error());
      }

      let flags = libc::fcntl(master, libc::F_GETFL, 0);
      if flags < 0 {
        return Err(std::io::Error::last_os_error());
      }
      if libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
        return Err(std::io::Error::last_os_error());
      }

      let pid = Pid::from_raw_unchecked(pid);
      let master = OwnedFd::from_raw_fd(master);

      UnixProcessesWaiter::wait_for(pid, on_wait_returned);

      Ok(UnixProcess {
        pid,
        master: AsyncFd::new(master)?,
      })
    }
  }
}

impl Process for UnixProcess {
  fn on_exited(&mut self) {}

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
