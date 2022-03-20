use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, spawn};
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use portable_pty::{ExitStatus, MasterPty};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;

pub struct Inst {
  pub vt: Arc<RwLock<vt100::Parser>>,

  pub pid: u32,
  pub master: Box<dyn MasterPty + Send>,
  pub killer: Box<dyn ChildKiller + Send + Sync>,

  pub running: Arc<AtomicBool>,
  pub on_exit: oneshot::Receiver<Option<ExitStatus>>,
}

impl Inst {
  pub fn spawn(
    cmd: CommandBuilder,
    tx: Sender<()>,
    size: (u16, u16),
  ) -> anyhow::Result<Self> {
    let vt = vt100::Parser::new(size.0, size.1, 1000);
    let vt = Arc::new(RwLock::new(vt));

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
      rows: size.0,
      cols: size.1,
      pixel_width: 0,
      pixel_height: 0,
    })?;

    let running = Arc::new(AtomicBool::new(true));
    let (tx_exit, on_exit) =
      tokio::sync::oneshot::channel::<Option<ExitStatus>>();
    let mut child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().unwrap();
    let killer = child.clone_killer();

    let mut reader = pair.master.try_clone_reader().unwrap();

    {
      let tx = tx.clone();
      let vt = vt.clone();
      let running = running.clone();
      spawn_blocking(move || {
        let mut buf = [0; 4 * 1024];
        loop {
          if !running.load(Ordering::Relaxed) {
            break;
          }

          match reader.read(&mut buf[..]) {
            Ok(count) => {
              if count > 0 {
                vt.clone().write().unwrap().process(&buf[..count]);
                match tx.blocking_send(()) {
                  Ok(_) => (),
                  Err(_) => break,
                }
              } else {
                thread::sleep(Duration::from_millis(10));
              }
            }
            _ => break,
          }
        }
      });
    }

    {
      let tx = tx.clone();
      let running = running.clone();
      spawn(move || {
        // Block until program exits
        let status = child.wait();
        running.store(false, Ordering::Relaxed);
        let _result = tx.send(());
        let _send_result = tx_exit.send(status.ok());
      });
    }

    let inst = Inst {
      vt,

      pid,
      master: pair.master,
      killer,

      running,
      on_exit,
    };
    Ok(inst)
  }

  pub fn resize(&self, rows: u16, cols: u16) {
    self
      .master
      .resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
      })
      .unwrap();

    self.vt.write().unwrap().set_size(rows, cols);
  }
}

pub struct Proc {
  pub name: String,
  pub inst: Inst,
}

impl Proc {
  pub fn new(
    name: String,
    cmd: CommandBuilder,
    tx: Sender<()>,
    size: (u16, u16),
  ) -> Self {
    Proc {
      name,
      inst: Inst::spawn(cmd, tx, size).unwrap(),
    }
  }

  pub async fn wait(self) {
    let _res = self.inst.on_exit.await;
  }

  pub fn is_up(&mut self) -> bool {
    self.inst.running.load(Ordering::Relaxed)
  }

  pub fn term(&mut self) {
    if self.is_up() {
      unsafe { libc::kill(self.inst.pid as i32, libc::SIGTERM) };
    }
  }

  pub fn kill(&mut self) {
    if self.is_up() {
      let _result = self.inst.killer.kill();
    }
  }
}
