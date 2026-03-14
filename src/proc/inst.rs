use std::fmt::Debug;

use tokio::sync::mpsc::UnboundedSender;

use crate::error::ResultLogger;
use crate::kernel::kernel_message::SharedVt;
use crate::kernel::proc::ProcId;
use crate::proc_log_config::{LogConfig, LogMode};
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::process::NativeProcess;
use crate::term_types::winsize::Winsize;

use super::msg::ProcEvent;
use super::Size;

pub struct Inst {
  pub vt: SharedVt,
  pub log_writer: Option<tokio::fs::File>,

  pub pid: u32,
  pub process: NativeProcess,
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
  pub async fn spawn(
    id: ProcId,
    name: &str,
    spec: &ProcessSpec,
    tx: UnboundedSender<ProcEvent>,
    size: &Size,
    scrollback_len: usize,
    log: Option<&LogConfig>,
  ) -> anyhow::Result<Self> {
    let vt = crate::vt100::Parser::new(size.height, size.width, scrollback_len);
    let vt = SharedVt::new(vt);

    tx.send(ProcEvent::SetVt(Some(vt.clone()))).log_ignore();

    #[cfg(unix)]
    let process = {
      crate::process::unix_process::UnixProcess::spawn(
        id,
        spec,
        crate::term_types::winsize::Winsize {
          x: size.width,
          y: size.height,
          x_px: 0,
          y_px: 0,
        },
        {
          let tx = tx.clone();
          Box::new(move |wait_status| {
            let exit_code = wait_status.exit_status().unwrap_or(212);
            let _result = tx.send(ProcEvent::Exited(exit_code as u32));
          })
        },
      )?
    };
    #[cfg(unix)]
    let pid: i32 = process.pid.as_raw_nonzero().into();

    #[cfg(windows)]
    let process = {
      use anyhow::Context as _;

      crate::process::win_process::WinProcess::spawn(
        id,
        spec,
        crate::term_types::winsize::Winsize {
          x: size.width,
          y: size.height,
          x_px: 0,
          y_px: 0,
        },
        {
          let tx = tx.clone();
          Box::new(move |exit_code| {
            let exit_code = exit_code.unwrap_or(213);
            let _result = tx.send(ProcEvent::Exited(exit_code as u32));
          })
        },
      )
      .context("WinProcess::spawn")?
    };
    #[cfg(windows)]
    let pid: i32 = process.pid;

    let log_file = log.and_then(|log| log.file_path(name, id.0, pid as u32));
    let log_writer = match log_file {
      Some(path) => {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
          std::fs::create_dir_all(parent).log_ignore();
        }
        let append = log.is_some_and(|log| log.mode() == LogMode::Append);
        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).write(true).append(append);
        if !append {
          options.truncate(true);
        }
        options
          .open(&path)
          .await
          .map_err(|e| log::warn!("Failed to open log file {:?}: {}", path, e))
          .ok()
      }
      None => None,
    };

    tx.send(ProcEvent::Started).log_ignore();

    let inst = Inst {
      vt,
      log_writer,

      process,
      pid: pid as u32,
      exit_code: None,
      stdout_eof: false,
    };
    Ok(inst)
  }

  pub fn resize(&mut self, size: &Size) {
    let rows = size.height;
    let cols = size.width;

    self
      .process
      .resize(Winsize {
        x: size.width,
        y: size.height,
        x_px: 0,
        y_px: 0,
      })
      .log_ignore();

    if let Ok(mut vt) = self.vt.write() {
      vt.set_size(rows, cols);
    }
  }
}
