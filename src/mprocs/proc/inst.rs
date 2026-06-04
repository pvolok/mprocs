use std::fmt::Debug;

use tokio::sync::mpsc::UnboundedSender;

use crate::error::ResultLogger;
use crate::kernel::task::TaskId;
use crate::process::NativeProcess;
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::term::Winsize;

use super::Size;
use super::msg::ProcEvent;

pub struct Inst {
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
    id: TaskId,
    spec: &ProcessSpec,
    tx: UnboundedSender<ProcEvent>,
    size: &Size,
  ) -> anyhow::Result<Self> {
    #[cfg(unix)]
    let process = {
      crate::process::unix_process::UnixProcess::spawn(
        id,
        spec,
        Winsize {
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
        Winsize {
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

    tx.send(ProcEvent::Started).log_ignore();

    let inst = Inst {
      process,
      pid: pid as u32,
      exit_code: None,
      stdout_eof: false,
    };
    Ok(inst)
  }

  pub fn resize(&mut self, size: &Size) {
    self
      .process
      .resize(Winsize {
        x: size.width,
        y: size.height,
        x_px: 0,
        y_px: 0,
      })
      .log_ignore();
  }
}
