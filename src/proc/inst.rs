use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::thread::spawn;

use portable_pty::MasterPty;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::spawn_blocking;

use crate::error::ResultLogger;
use crate::kernel::kernel_message::SharedVt;
use crate::kernel::proc::ProcId;

use super::msg::ProcEvent;
use super::{ReplySender, Size};

pub struct Inst {
  pub vt: SharedVt,

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

impl Inst {
  pub fn spawn(
    id: ProcId,
    cmd: CommandBuilder,
    tx: UnboundedSender<ProcEvent>,
    size: &Size,
    scrollback_len: usize,
    log_file: Option<PathBuf>,
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
    let vt = SharedVt::new(vt);

    tx.send(ProcEvent::SetVt(Some(vt.clone()))).log_ignore();

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

    let _r = tx.send(ProcEvent::Started);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = pair.master.take_writer().unwrap();

    {
      let tx = tx.clone();
      let vt = vt.clone();
      spawn_blocking(move || {
        // Open log file if configured (truncate on process start)
        let mut log_writer: Option<BufWriter<std::fs::File>> =
          log_file.and_then(|path| {
            // Create parent directories if needed
            if let Some(parent) = path.parent() {
              let _ = std::fs::create_dir_all(parent);
            }
            OpenOptions::new()
              .create(true)
              .write(true)
              .truncate(true)
              .open(&path)
              .map(BufWriter::new)
              .map_err(|e| log::warn!("Failed to open log file {:?}: {}", path, e))
              .ok()
          });

        let mut buf = vec![0; 32 * 1024];
        loop {
          match reader.read(&mut buf[..]) {
            Ok(count) => {
              if count == 0 {
                break;
              }

              // Write to log file if configured
              if let Some(ref mut writer) = log_writer {
                let _ = writer.write_all(&buf[..count]);
                let _ = writer.flush();
              }

              if let Ok(mut vt) = vt.write() {
                vt.process(&buf[..count]);
                match tx.send(ProcEvent::Render) {
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
        let _ = tx.send(ProcEvent::StdoutEOF);
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
        let _result = tx.send(ProcEvent::Exited(exit_code));
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
