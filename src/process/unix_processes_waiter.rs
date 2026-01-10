use std::{collections::HashMap, sync::Mutex};

use anyhow::{anyhow, bail};
use rustix::{
  process::{WaitOptions, WaitStatus},
  termios::Pid,
};

pub struct UnixProcessesWaiter {
  sig_id: signal_hook::SigId,
  listeners: HashMap<Pid, Box<dyn Fn(WaitStatus) + Send + Sync>>,
  unclaimed: HashMap<Pid, WaitStatus>,
}

static GLOBAL: Mutex<Option<UnixProcessesWaiter>> = Mutex::new(None);

impl UnixProcessesWaiter {
  pub fn wait_for(pid: Pid, f: Box<dyn Fn(WaitStatus) + Send + Sync>) {
    match GLOBAL.lock() {
      Ok(mut guard) => {
        if let Some(pw) = guard.as_mut() {
          match pw.unclaimed.remove(&pid) {
            Some(wait_status) => {
              f(wait_status);
            }
            None => {
              pw.listeners.insert(pid, f);
            }
          }
        }
      }
      Err(_) => (),
    }
  }

  pub fn init() -> anyhow::Result<()> {
    let mut holder =
      GLOBAL.lock().map_err(|_e| anyhow!("Mutex is poisoned."))?;
    if holder.is_some() {
      bail!("UnixProcessWaiter is already initialized.");
    }
    let sig_id = unsafe {
      signal_hook::low_level::register(signal_hook::consts::SIGCHLD, || {
        loop {
          match rustix::process::wait(WaitOptions::NOHANG) {
            Ok(Some((pid, wait_status))) => match GLOBAL.lock() {
              Ok(mut guard) => {
                let pw = guard.as_mut().unwrap();
                match pw.listeners.remove(&pid) {
                  Some(listener) => {
                    listener(wait_status);
                  }
                  None => {
                    pw.unclaimed.insert(pid, wait_status);
                  }
                }
              }
              Err(e) => {
                log::error!("SIGCHLD signal init error: {}", e);
              }
            },
            Ok(None) => break,
            Err(e) => {
              // ECHILD - No spawned processes.
              if e.raw_os_error() != libc::ECHILD {
                log::error!(
                  "ProcessesWaiter wait() error: {} ({})",
                  e.kind(),
                  e.raw_os_error()
                );
              }
              break;
            }
          }
        }
      })?
    };
    *holder = Some(UnixProcessesWaiter {
      sig_id,
      listeners: Default::default(),
      unclaimed: Default::default(),
    });

    Ok(())
  }

  pub fn uninit() -> anyhow::Result<()> {
    let mut holder =
      GLOBAL.lock().map_err(|_e| anyhow!("Mutex is poisoned."))?;
    match holder.as_mut() {
      Some(pw) => {
        signal_hook::low_level::unregister(pw.sig_id);
      }
      None => bail!("Cannot uninit None UnixProcessWaiter."),
    }
    *holder = None;

    Ok(())
  }
}
