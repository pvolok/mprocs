use std::sync::{Arc, RwLock};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use portable_pty::{ExitStatus, MasterPty};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncReadExt, DuplexStream};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver;
use tokio::task::spawn_blocking;

pub struct Inst {
  pub vt: Arc<RwLock<vt100::Parser>>,
  pub master: Box<dyn MasterPty + Send>,
}

impl Inst {
  pub fn spawn(
    cmd: CommandBuilder,
    tx: Sender<()>,
    size: (u16, u16),
  ) -> anyhow::Result<Self> {
    let mut vt = vt100::Parser::new(size.0, size.1, 1000);
    vt.process(b"this text is \x1b[31mRED\x1b[m");
    let vt = Arc::new(RwLock::new(vt));

    let pty_system = native_pty_system();

    let pair = pty_system.openpty(PtySize {
      rows: size.0,
      cols: size.1,
      pixel_width: 0,
      pixel_height: 0,
    })?;

    let (tx_exit, rx_exit) =
      tokio::sync::oneshot::channel::<Option<ExitStatus>>();
    let slave = pair.slave;
    let child = spawn_blocking(move || slave.spawn_command(cmd));
    tokio::spawn(async move {
      let status = async move {
        let mut child = child.await.ok()?.ok()?;

        // Block until program exits
        let status = spawn_blocking(move || child.wait()).await.ok()?.ok()?;
        Some(status)
      };
      let status = status.await;
      let _send_result = tx_exit.send(status);
    });

    let master = pair.master;
    let mut reader = master.try_clone_reader().unwrap();
    let mut writer = master.try_clone_writer().unwrap();

    let vt1 = vt.clone();
    spawn_blocking(move || {
      let mut buf = [0; 256];
      loop {
        match reader.read(&mut buf) {
          Ok(count) => {
            vt1.write().unwrap().process(&buf[..count]);
            tx.blocking_send(()).unwrap();
          }
          _ => break,
        }
      }
    });

    let inst = Inst { vt, master };
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
  pub fn new(name: String, tx: Sender<()>, size: (u16, u16)) -> Self {
    Proc {
      name,
      inst: Inst::spawn(CommandBuilder::new("top"), tx, size).unwrap(),
    }
  }
}
