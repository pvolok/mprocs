use std::future::pending;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::error::ResultLogger;
use crate::kernel::kernel_message::{KernelCommand, SharedVt, TaskContext};
use crate::kernel::task::{TaskCmd, TaskDef};
use crate::kernel::task_path::TaskPath;
use crate::kernel::task_screen::{TaskScreen, TaskScreenCmd, TaskScreenEffect};
use crate::process::NativeProcess;
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::term::{Parser, Winsize};

struct ProcExited(u32);

pub fn spawn_proc_task(
  parent: &TaskContext,
  task_path: TaskPath,
  spec: ProcessSpec,
) {
  let vt = SharedVt::new(Parser::new(24, 80, 1000));
  let task_vt = vt.clone();
  let task_id = parent.alloc_id();
  parent.spawn_async_with_id(
    task_id,
    TaskDef {
      stop_on_quit: true,
      path: Some(task_path),
      vt: Some(vt),
      ..Default::default()
    },
    move |ctx, receiver| async move {
      proc_main(ctx, receiver, spec, task_vt).await;
    },
  );
}

async fn proc_main(
  ctx: TaskContext,
  mut receiver: UnboundedReceiver<TaskCmd>,
  spec: ProcessSpec,
  vt: SharedVt,
) {
  let mut process = match spawn_native(
    &ctx,
    &spec,
    Winsize {
      x: 80,
      y: 24,
      x_px: 0,
      y_px: 0,
    },
  ) {
    Ok(p) => p,
    Err(err) => {
      log::warn!("Process spawn error: {}", err);
      ctx.send(KernelCommand::TaskStopped(255));
      return;
    }
  };

  ctx.send(KernelCommand::TaskStarted);

  let mut task_screen = TaskScreen::new(ctx.task_id, vt);
  let mut screen_effects: Vec<TaskScreenEffect> = Vec::new();
  let mut read_buf = [0u8; 8 * 1024];
  let mut stdout_eof = false;
  let mut exit_code: Option<u32> = None;

  loop {
    enum Next {
      Cmd(Option<TaskCmd>),
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
      cmd = receiver.recv() => Next::Cmd(cmd),
      n = read_fut => Next::Read(n),
    };

    match next {
      Next::Cmd(None) => break,
      Next::Cmd(Some(cmd)) => match cmd {
        TaskCmd::Start => {}
        TaskCmd::Stop | TaskCmd::Kill => {
          process.kill().await.log_ignore();
        }
        TaskCmd::Msg(msg) => {
          let msg = match msg.downcast::<ProcExited>() {
            Ok(exited) => {
              exit_code = Some(exited.0);
              if stdout_eof {
                ctx.send(KernelCommand::TaskStopped(exited.0));
              }
              continue;
            }
            Err(msg) => msg,
          };
          let msg = match msg.downcast::<TaskScreenCmd>() {
            Ok(cmd) => {
              task_screen.handle_cmd(*cmd, &mut screen_effects);
              apply_effects(
                &mut screen_effects,
                &mut process,
                task_screen.vt(),
              )
              .await;
              continue;
            }
            Err(msg) => msg,
          };
          let _ = msg;
          log::error!("ProcTask received unknown Msg");
        }
      },

      Next::Read(Ok(0)) => {
        stdout_eof = true;
        if let Some(code) = exit_code {
          ctx.send(KernelCommand::TaskStopped(code));
        }
      }
      Next::Read(Ok(n)) => {
        task_screen.process(&read_buf[..n], &mut screen_effects);
        apply_effects(&mut screen_effects, &mut process, task_screen.vt())
          .await;
      }
      Next::Read(Err(e)) => {
        log::warn!("Process read error: {}", e);
        stdout_eof = true;
        if let Some(code) = exit_code {
          ctx.send(KernelCommand::TaskStopped(code));
        }
      }
    }
  }
}

async fn apply_effects(
  effects: &mut Vec<TaskScreenEffect>,
  process: &mut NativeProcess,
  vt: &SharedVt,
) {
  for effect in effects.drain(..) {
    match effect {
      TaskScreenEffect::Reply(s) => {
        process.write_all(s.as_bytes()).await.log_ignore();
      }
      TaskScreenEffect::Resize(size) => {
        if let Ok(mut parser) = vt.write() {
          parser.set_size(size.y, size.x);
        }
        process.resize(size).log_ignore();
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
        exit_ctx.send_self_custom(ProcExited(code));
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
        exit_ctx.send_self_custom(ProcExited(code));
      }),
    )
    .context("WinProcess::spawn")
  }
}
