use std::{
  ffi::CString,
  os::fd::{FromRawFd, OwnedFd},
  ptr::{null, null_mut},
};

use rustix::{process::WaitStatus, termios::Pid};
use tokio::io::unix::AsyncFd;

use crate::{
  kernel::task::TaskId,
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
    _id: TaskId,
    spec: &ProcessSpec,
    size: Winsize,
    on_wait_returned: Box<dyn Fn(WaitStatus) + Send + Sync>,
  ) -> std::io::Result<Self> {
    let prog = CString::new(spec.prog.as_str()).unwrap_or_default();

    let mut argv: Vec<CString> = Vec::new();
    argv.push(prog.clone());
    for arg in &spec.args {
      argv.push(CString::new(arg.as_str()).unwrap_or_default());
    }
    let argv_ptrs = {
      let mut v: Vec<*const libc::c_char> =
        argv.iter().map(|a| a.as_ptr()).collect();
      v.push(null());
      v
    };

    let cwd_c = spec
      .get_cwd()
      .as_ref()
      .map(|cwd| CString::new(cwd.as_str()).unwrap_or_default());

    let env_c: Vec<(CString, Option<CString>)> = spec
      .env
      .iter()
      .filter_map(|(k, v)| {
        let k_c = CString::new(k.as_str()).ok()?;
        let v_c = v.as_ref().and_then(|v| CString::new(v.as_str()).ok());
        Some((k_c, v_c))
      })
      .collect();

    unsafe {
      let mut block_set: libc::sigset_t = std::mem::zeroed();
      let mut old_set: libc::sigset_t = std::mem::zeroed();
      libc::sigfillset(&mut block_set);
      libc::pthread_sigmask(libc::SIG_SETMASK, &block_set, &mut old_set);

      let mut empty_set: libc::sigset_t = std::mem::zeroed();
      libc::sigemptyset(&mut empty_set);

      let mut master_fd = -1;
      // Some args are *mut on some BSD variants.
      #[allow(clippy::unnecessary_mut_passed)]
      let pid = libc::forkpty(
        &mut master_fd,
        null_mut(),
        null_mut(),
        &mut size.into(),
      );
      if pid < 0 {
        libc::pthread_sigmask(libc::SIG_SETMASK, &old_set, null_mut());
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
        libc::pthread_sigmask(libc::SIG_SETMASK, &empty_set, null_mut());

        if let Some(cwd) = &cwd_c {
          if libc::chdir(cwd.as_ptr()) != 0 {
            libc::_exit(1);
          }
        }

        for (key, value) in &env_c {
          match value {
            Some(v) => {
              libc::setenv(key.as_ptr(), v.as_ptr(), 1);
            }
            None => {
              libc::unsetenv(key.as_ptr());
            }
          }
        }

        libc::execvp(prog.as_ptr(), argv_ptrs.as_ptr());
        libc::perror(null());
        libc::_exit(1);
      }

      libc::pthread_sigmask(libc::SIG_SETMASK, &old_set, null_mut());

      let flags = libc::fcntl(master_fd, libc::F_GETFD, 0);
      if flags < 0 {
        return Err(std::io::Error::last_os_error());
      }
      if libc::fcntl(master_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) < 0 {
        return Err(std::io::Error::last_os_error());
      }

      let flags = libc::fcntl(master_fd, libc::F_GETFL, 0);
      if flags < 0 {
        return Err(std::io::Error::last_os_error());
      }
      if libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
        return Err(std::io::Error::last_os_error());
      }

      let pid = Pid::from_raw_unchecked(pid);
      let master = OwnedFd::from_raw_fd(master_fd);

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
