use std::fmt::Debug;
use std::io::Write;
use std::sync::{Arc, RwLock};
use std::thread::spawn;

use portable_pty::MasterPty;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::spawn_blocking;

use crate::error::ResultLogger;

use super::msg::ProcEvent;
use super::{ReplySender, Size};

pub struct Inst {
  pub vt: VtWrap,

  pub pid: u32,
  pub master: Option<Box<dyn MasterPty + Send>>,
  pub writer: Box<dyn Write + Send>,
  pub killer: Box<dyn ChildKiller + Send + Sync>,

  pub exit_code: Option<u32>,
  pub stdout_eof: bool,
}

impl Debug for Inst {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Inst")
      .field("pid", &self.pid)
      .field("exited", &self.exit_code)
      .field("stdout_eof", &self.stdout_eof)
      .finish()
  }
}

pub type VtWrap = Arc<RwLock<crate::vt100::Parser<ReplySender>>>;

impl Inst {
  pub fn spawn(
    id: usize,
    cmd: CommandBuilder,
    tx: UnboundedSender<(usize, ProcEvent)>,
    size: &Size,
    scrollback_len: usize,
  ) -> anyhow::Result<Self> {
    let vt = crate::vt100::Parser::new(
      size.height,
      size.width,
      scrollback_len,
      ReplySender {
        proc_id: id,
        sender: tx.clone(),
      },
    );
    let vt = Arc::new(RwLock::new(vt));

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
      rows: size.height,
      cols: size.width,
      pixel_width: 0,
      pixel_height: 0,
    })?;

    let mut child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().unwrap_or(0);
    let killer = child.clone_killer();

    let _r = tx.send((id, ProcEvent::Started));

    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = pair.master.take_writer().unwrap();

    {
      let tx = tx.clone();
      let vt = vt.clone();
      spawn_blocking(move || {
        let mut buf = vec![0; 32 * 1024];
        loop {
          match reader.read(&mut buf[..]) {
            Ok(count) => {
              if count == 0 {
                break;
              }
              if let Ok(mut vt) = vt.write() {
                vt.process(&buf[..count]);
                match tx.send((id, ProcEvent::Render)) {
                  Ok(_) => (),
                  Err(err) => {
                    log::debug!("Proc read error: ({:?})", err);
                    break;
                  }
                }
              }
            }
            _ => break,
          }
        }
        let _ = tx.send((id, ProcEvent::StdoutEOF));
      });
    }

    {
      let tx = tx.clone();
      spawn(move || {
        // Block until program exits
        let exit_code = match child.wait() {
          Ok(status) => status.exit_code(),
          Err(_e) => 211,
        };
        let _result = tx.send((id, ProcEvent::Exited(exit_code)));
      });
    }

    let inst = Inst {
      vt,

      pid,
      master: Some(pair.master),
      writer,
      killer,

      exit_code: None,
      stdout_eof: false,
    };
    Ok(inst)
  }

  pub fn resize(&self, size: &Size) {
    let rows = size.height;
    let cols = size.width;

    if let Some(master) = &self.master {
      master
        .resize(PtySize {
          rows,
          cols,
          pixel_width: 0,
          pixel_height: 0,
        })
        .log_ignore();
    }

    if let Ok(mut vt) = self.vt.write() {
      vt.set_size(rows, cols);
    }
  }
}
