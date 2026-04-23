use std::future::pending;

use tokio::sync::mpsc::UnboundedSender;

use crate::error::ResultLogger;
use crate::kernel::kernel_message::{KernelCommand, SharedVt, TaskContext};
use crate::kernel::task::{Effects, Task, TaskCmd, TaskDef};
use crate::kernel::task_path::TaskPath;
use crate::process::NativeProcess;
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::term::{Parser, VtEvent, Winsize};

// Messages from async worker -> in-kernel ProcTask

struct WorkerExited(u32);
struct WorkerStdoutEof;

// Commands from in-kernel ProcTask -> async worker

enum WorkerCmd {
  Kill,
}

pub struct ProcTask {
  worker_tx: UnboundedSender<WorkerCmd>,
  exit_code: Option<u32>,
  stdout_eof: bool,
}

impl ProcTask {
  pub fn spawn(parent: &TaskContext, task_path: TaskPath, spec: ProcessSpec) {
    parent.register_with_id(
      parent.alloc_id(),
      TaskDef {
        stop_on_quit: true,
        path: Some(task_path),
        ..Default::default()
      },
      Box::new(move |ctx| {
        let (worker_tx, worker_rx) = tokio::sync::mpsc::unbounded_channel();

        let vt = SharedVt::new(Parser::new(24, 80, 1000));

        let worker_ctx = ctx.clone();
        let worker_vt = vt.clone();
        tokio::spawn(async move {
          proc_worker(worker_ctx, spec, worker_vt, worker_rx).await;
        });

        ctx.send(KernelCommand::TaskUpdatedScreen(Some(vt)));
        ctx.send(KernelCommand::TaskStarted);

        Box::new(ProcTask {
          worker_tx,
          exit_code: None,
          stdout_eof: false,
        })
      }),
    );
  }

  fn maybe_stopped(&self, fx: &mut Effects) {
    if let Some(code) = self.exit_code {
      if self.stdout_eof {
        fx.stopped(code);
      }
    }
  }
}

impl Task for ProcTask {
  fn handle_cmd(&mut self, cmd: TaskCmd, fx: &mut Effects) {
    match cmd {
      TaskCmd::Start => {}
      TaskCmd::Stop | TaskCmd::Kill => {
        let _ = self.worker_tx.send(WorkerCmd::Kill);
      }
      TaskCmd::Msg(msg) => {
        let msg = match msg.downcast::<WorkerExited>() {
          Ok(exited) => {
            self.exit_code = Some(exited.0);
            self.maybe_stopped(fx);
            return;
          }
          Err(msg) => msg,
        };
        if msg.downcast::<WorkerStdoutEof>().is_ok() {
          self.stdout_eof = true;
          self.maybe_stopped(fx);
        }
      }
    }
  }
}

async fn proc_worker(
  ctx: TaskContext,
  spec: ProcessSpec,
  vt: SharedVt,
  mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<WorkerCmd>,
) {
  let width: u16 = 80;
  let height: u16 = 24;

  let mut process = match spawn_native(
    &ctx,
    &spec,
    Winsize {
      x: width,
      y: height,
      x_px: 0,
      y_px: 0,
    },
  ) {
    Ok(p) => p,
    Err(err) => {
      log::error!("Process spawn error: {}", err);
      ctx.send_self_custom(WorkerExited(255));
      ctx.send_self_custom(WorkerStdoutEof);
      return;
    }
  };

  let mut read_buf = [0u8; 8 * 1024];
  let mut vt_events = Vec::new();
  let mut stdout_eof = false;

  loop {
    enum Next {
      Cmd(Option<WorkerCmd>),
      Read(std::io::Result<usize>),
    }

    let read_fut = async {
      if stdout_eof {
        pending().await
      } else {
        process.read(&mut read_buf).await
      }
    };

    let next = tokio::select! {
      cmd = cmd_rx.recv() => Next::Cmd(cmd),
      n = read_fut => Next::Read(n),
    };

    match next {
      Next::Cmd(Some(WorkerCmd::Kill)) => {
        process.kill().await.log_ignore();
      }
      Next::Cmd(None) => break,

      Next::Read(Ok(0)) => {
        stdout_eof = true;
        ctx.send_self_custom(WorkerStdoutEof);
      }
      Next::Read(Ok(n)) => {
        let bytes = &read_buf[..n];
        if let Ok(mut parser) = vt.write() {
          parser.screen.process(bytes, &mut vt_events);
          drop(parser);
        }
        for ev in vt_events.drain(..) {
          if let VtEvent::Reply(s) = ev {
            process.write_all(s.as_bytes()).await.log_ignore();
          }
        }
        ctx.send(KernelCommand::TaskRendered);
      }
      Next::Read(Err(e)) => {
        log::error!("Process read error: {}", e);
        stdout_eof = true;
        ctx.send_self_custom(WorkerStdoutEof);
      }
    }
  }
}

fn spawn_native(
  ctx: &TaskContext,
  spec: &ProcessSpec,
  size: Winsize,
) -> anyhow::Result<NativeProcess> {
  let exit_ctx = ctx.clone();

  #[cfg(unix)]
  {
    Ok(crate::process::unix_process::UnixProcess::spawn(
      ctx.task_id,
      spec,
      size,
      Box::new(move |wait_status| {
        let code = wait_status.exit_status().unwrap_or(212) as u32;
        exit_ctx.send_self_custom(WorkerExited(code));
      }),
    )?)
  }

  #[cfg(windows)]
  {
    use anyhow::Context as _;
    crate::process::win_process::WinProcess::spawn(
      ctx.task_id,
      spec,
      size,
      Box::new(move |exit_code| {
        let code = exit_code.unwrap_or(213) as u32;
        exit_ctx.send_self_custom(WorkerExited(code));
      }),
    )
    .context("WinProcess::spawn")
  }
}
