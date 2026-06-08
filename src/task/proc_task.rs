use std::future::pending;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::error::ResultLogger;
use crate::kernel::kernel_message::{KernelCommand, SharedVt, TaskContext};
use crate::kernel::task::{TaskCmd, TaskDef, TaskId};
use crate::kernel::task_path::TaskPath;
use crate::kernel::task_screen::{TaskScreen, TaskScreenCmd, TaskScreenEffect};
use crate::process::NativeProcess;
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::task::logger::{LogResolver, spawn_logger};
use crate::term::encode::{KeyCodeEncodeModes, encode_key};
use crate::term::key::Key;
use crate::term::{Parser, Winsize};

struct ProcExited(u32);

pub struct ProcInput(pub Key);

pub struct DuplicateProc(pub Option<String>);

/// How a proc task should react to `Stop` (`Kill` is always a hard kill).
#[derive(Clone, Debug, Default)]
pub enum StopSignal {
  SIGINT,
  #[default]
  SIGTERM,
  SIGKILL,
  SendKeys(Vec<Key>),
  HardKill,
  /// Run a shell command as the stop action. Useful for tools like
  /// `podman compose` that don't reliably respond to signals but do have
  /// an explicit teardown command (e.g. `podman compose down`). The main
  /// process is expected to exit on its own once the stop command
  /// completes (e.g. `compose up` exits when containers go away).
  Cmd(String),
}

pub struct ProcTaskConfig {
  pub spec: ProcessSpec,
  pub label: Option<String>,
  pub stop: StopSignal,
  pub log: Option<LogResolver>,
  pub autostart: bool,
  pub autorestart: bool,
  pub scrollback_len: usize,
  pub mouse_scroll_speed: usize,
  pub deps: Vec<TaskId>,
}

impl ProcTaskConfig {
  pub fn new(spec: ProcessSpec) -> Self {
    Self {
      spec,
      label: None,
      stop: StopSignal::default(),
      log: None,
      autostart: true,
      autorestart: false,
      scrollback_len: 1000,
      mouse_scroll_speed: 5,
      deps: Vec::new(),
    }
  }
}

pub fn spawn_proc_task(
  parent: &TaskContext,
  task_path: Option<TaskPath>,
  config: ProcTaskConfig,
) -> TaskId {
  let task_id = parent.alloc_id();
  spawn_proc_task_with_id(parent, task_id, task_path, config);
  task_id
}

pub fn spawn_proc_task_with_id(
  parent: &TaskContext,
  task_id: TaskId,
  task_path: Option<TaskPath>,
  config: ProcTaskConfig,
) {
  let ProcTaskConfig {
    spec,
    stop,
    log,
    autostart,
    autorestart,
    scrollback_len,
    mouse_scroll_speed,
    deps,
    label,
  } = config;
  let vt = SharedVt::new(Parser::new(24, 80, scrollback_len));
  let task_vt = vt.clone();
  parent.spawn_async_with_id(
    task_id,
    TaskDef {
      stop_on_quit: true,
      autostart,
      autorestart,
      deps,
      path: task_path,
      label,
      vt: Some(vt),
      ..Default::default()
    },
    move |ctx, receiver| async move {
      proc_main(
        ctx,
        receiver,
        spec,
        task_vt,
        log,
        stop,
        scrollback_len,
        mouse_scroll_speed,
        autorestart,
      )
      .await;
    },
  );
}

async fn proc_main(
  ctx: TaskContext,
  mut receiver: UnboundedReceiver<TaskCmd>,
  spec: ProcessSpec,
  vt: SharedVt,
  mut log: Option<LogResolver>,
  stop: StopSignal,
  scrollback_len: usize,
  mouse_scroll_speed: usize,
  autorestart: bool,
) {
  let mut task_screen = TaskScreen::new(ctx.task_id, vt, mouse_scroll_speed);
  let mut screen_effects: Vec<TaskScreenEffect> = Vec::new();

  let mut process: Option<NativeProcess> = None;
  // The log path is resolved per spawn (it may contain the pid).
  let mut current_log: Option<(std::path::PathBuf, u64)> = None;
  let mut read_buf = [0u8; 8 * 1024];
  let mut stdout_eof = false;
  let mut exit_code: Option<u32> = None;

  loop {
    if stdout_eof
      && let Some(code) = exit_code
      && process.take().is_some()
    {
      ctx.send(KernelCommand::TaskStopped(code));
    }

    enum Next {
      Cmd(Option<TaskCmd>),
      Read(std::io::Result<usize>),
    }
    let read_fut = async {
      match process.as_mut() {
        Some(p) if !stdout_eof => p.read(&mut read_buf).await,
        _ => pending().await,
      }
    };
    let next = tokio::select! {
      cmd = receiver.recv() => Next::Cmd(cmd),
      n = read_fut => Next::Read(n),
    };

    match next {
      Next::Cmd(None) => break,
      Next::Cmd(Some(cmd)) => match cmd {
        TaskCmd::Start => {
          if process.is_none() {
            process = start_instance(&ctx, &spec, task_screen.vt());
            if let Some(p) = &process {
              exit_code = None;
              stdout_eof = false;
              update_log_observer(
                &mut task_screen,
                &mut log,
                &mut current_log,
                p.pid(),
              );
            }
          }
        }
        TaskCmd::Stop => {
          if let Some(p) = process.as_mut() {
            stop_process(p, &stop, task_screen.vt(), &spec).await;
          }
        }
        TaskCmd::Kill => {
          if let Some(p) = process.as_mut() {
            p.kill().await.log_ignore();
          }
        }
        TaskCmd::Msg(msg) => {
          let msg = match msg.downcast::<ProcExited>() {
            Ok(exited) => {
              exit_code = Some(exited.0);
              if let Some(p) = process.as_mut() {
                p.on_exited();
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
          let msg = match msg.downcast::<ProcInput>() {
            Ok(input) => {
              if let Some(p) = process.as_mut() {
                send_key(p, task_screen.vt(), input.0).await;
              }
              continue;
            }
            Err(msg) => msg,
          };
          let msg = match msg.downcast::<DuplicateProc>() {
            Ok(dup) => {
              let new_id = ctx.alloc_id();
              let path = TaskPath::new(format!("/{}", new_id.0)).ok();
              spawn_proc_task_with_id(
                &ctx,
                new_id,
                path,
                ProcTaskConfig {
                  spec: spec.clone(),
                  stop: stop.clone(),
                  log: None,
                  autostart: true,
                  autorestart,
                  scrollback_len,
                  mouse_scroll_speed,
                  deps: Vec::new(),
                  label: dup.0,
                },
              );
              continue;
            }
            Err(msg) => msg,
          };
          let _ = msg;
          log::error!("ProcTask received unknown Msg");
        }
      },

      Next::Read(Ok(0)) => stdout_eof = true,
      Next::Read(Ok(n)) => {
        task_screen
          .process(&read_buf[..n], &mut screen_effects)
          .await;
        apply_effects(&mut screen_effects, &mut process, task_screen.vt())
          .await;
      }
      Next::Read(Err(e)) => {
        log::warn!("Process read error: {}", e);
        stdout_eof = true;
      }
    }
  }
}

fn update_log_observer(
  task_screen: &mut TaskScreen,
  log: &mut Option<LogResolver>,
  current: &mut Option<(std::path::PathBuf, u64)>,
  pid: u32,
) {
  let Some(resolve) = log.as_mut() else {
    return;
  };
  let Some(sink) = resolve(pid) else {
    return;
  };
  if let Some((path, _)) = current {
    if *path == sink.path {
      return;
    }
  }
  if let Some((_, id)) = current.take() {
    task_screen.remove_direct_observer(id);
  }
  let path = sink.path.clone();
  let id = task_screen.add_direct_observer(spawn_logger(sink));
  *current = Some((path, id));
}

fn start_instance(
  ctx: &TaskContext,
  spec: &ProcessSpec,
  vt: &SharedVt,
) -> Option<NativeProcess> {
  let size = match vt.read() {
    Ok(parser) => {
      let s = parser.screen().size();
      Winsize {
        x: s.width,
        y: s.height,
        x_px: 0,
        y_px: 0,
      }
    }
    Err(_) => Winsize {
      x: 80,
      y: 24,
      x_px: 0,
      y_px: 0,
    },
  };
  if let Ok(mut parser) = vt.write() {
    parser.reset();
    parser.set_size(size.y, size.x);
  }
  match spawn_native(ctx, spec, size) {
    Ok(process) => {
      ctx.send(KernelCommand::TaskStarted);
      Some(process)
    }
    Err(err) => {
      log::warn!("Process spawn error: {}", err);
      ctx.send(KernelCommand::TaskStopped(255));
      None
    }
  }
}

async fn apply_effects(
  effects: &mut Vec<TaskScreenEffect>,
  process: &mut Option<NativeProcess>,
  vt: &SharedVt,
) {
  for effect in effects.drain(..) {
    match effect {
      TaskScreenEffect::Write(s) => {
        if let Some(p) = process.as_mut() {
          p.write_all(s.as_bytes()).await.log_ignore();
        }
      }
      TaskScreenEffect::Resize(size) => {
        if let Ok(mut parser) = vt.write() {
          parser.set_size(size.y, size.x);
        }
        if let Some(p) = process.as_mut() {
          p.resize(size).log_ignore();
        }
      }
    }
  }
}

async fn send_key(process: &mut NativeProcess, vt: &SharedVt, key: Key) {
  let application_cursor_keys = vt
    .read()
    .map(|parser| parser.screen().application_cursor())
    .unwrap_or(false);
  let modes = KeyCodeEncodeModes {
    enable_csi_u_key_encoding: true,
    application_cursor_keys,
    newline_mode: false,
  };
  match encode_key(&key, modes) {
    Ok(encoded) => process.write_all(encoded.as_bytes()).await.log_ignore(),
    Err(_) => log::warn!("Failed to encode key: {}", key.spec()),
  }
}

#[cfg(not(windows))]
async fn stop_process(
  process: &mut NativeProcess,
  stop: &StopSignal,
  vt: &SharedVt,
  spec: &ProcessSpec,
) {
  match stop {
    StopSignal::SIGINT => process.send_signal(libc::SIGINT).log_ignore(),
    StopSignal::SIGTERM => process.send_signal(libc::SIGTERM).log_ignore(),
    StopSignal::SIGKILL => process.send_signal(libc::SIGKILL).log_ignore(),
    StopSignal::SendKeys(keys) => {
      for key in keys {
        send_key(process, vt, key.clone()).await;
      }
    }
    StopSignal::HardKill => process.kill().await.log_ignore(),
    StopSignal::Cmd(shell) => run_stop_cmd(spec, shell.clone()),
  }
}

#[cfg(windows)]
async fn stop_process(
  process: &mut NativeProcess,
  stop: &StopSignal,
  vt: &SharedVt,
  spec: &ProcessSpec,
) {
  match stop {
    StopSignal::SIGINT => log::debug!("SIGINT signal is ignored on Windows"),
    StopSignal::SIGTERM | StopSignal::SIGKILL | StopSignal::HardKill => {
      process.kill().await.log_ignore()
    }
    StopSignal::SendKeys(keys) => {
      for key in keys {
        send_key(process, vt, key.clone()).await;
      }
    }
    StopSignal::Cmd(shell) => run_stop_cmd(spec, shell.clone()),
  }
}

fn run_stop_cmd(spec: &ProcessSpec, shell: String) {
  let cwd = spec.cwd.clone();
  let env = spec.env.clone();
  tokio::spawn(async move {
    #[cfg(windows)]
    let mut cmd = {
      let mut c = tokio::process::Command::new("pwsh.exe");
      c.arg("-Command").arg(&shell);
      c
    };
    #[cfg(not(windows))]
    let mut cmd = {
      let mut c = tokio::process::Command::new("/bin/sh");
      c.arg("-c").arg(&shell);
      c
    };
    if let Some(cwd) = &cwd {
      cmd.current_dir(cwd);
    }
    for (k, v) in &env {
      match v {
        Some(v) => {
          cmd.env(k, v);
        }
        None => {
          cmd.env_remove(k);
        }
      }
    }
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    if let Err(e) = cmd.status().await {
      log::warn!("Stop command failed: {}", e);
    }
  });
}

#[cfg(not(windows))]
#[cfg(test)]
mod tests {
  use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

  use crate::kernel::kernel::Kernel;
  use crate::kernel::kernel_message::{
    KernelCommand, KernelQuery, KernelQueryResponse, TaskContext,
  };
  use crate::kernel::task::TaskId;
  use crate::task::logger::LogSink;

  use super::*;

  async fn resolve(pc: &TaskContext, path: &str) -> TaskId {
    let (tx, rx) = tokio::sync::oneshot::channel();
    pc.send(KernelCommand::Query(
      KernelQuery::ResolvePath(TaskPath::new(path).unwrap()),
      tx,
    ));
    let resp = tokio::time::timeout(Duration::from_secs(1), rx)
      .await
      .expect("timed out resolving path")
      .expect("kernel query channel closed");
    match resp {
      KernelQueryResponse::ResolvedPath(Some(id)) => id,
      _ => panic!("path did not resolve: {path}"),
    }
  }

  #[tokio::test]
  async fn proc_output_is_logged_via_direct_observer() {
    let nanos = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    let mut log_path = std::env::temp_dir();
    log_path.push(format!("mprocs_log_{}_{}.log", std::process::id(), nanos));

    let kernel = Kernel::new();
    let pc = kernel.context();

    let path = TaskPath::new("/logged").unwrap();
    let spec = ProcessSpec::from_argv(vec![
      "sh".to_string(),
      "-c".to_string(),
      "printf hello-log".to_string(),
    ]);
    let sink_path = log_path.clone();
    spawn_proc_task(
      &pc,
      Some(path),
      ProcTaskConfig {
        log: Some(Box::new(move |_pid| {
          Some(LogSink {
            path: sink_path.clone(),
            append: false,
          })
        })),
        ..ProcTaskConfig::new(spec)
      },
    );

    let kernel_task = tokio::spawn(kernel.run());

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
      if let Ok(contents) = std::fs::read_to_string(&log_path) {
        if contents.contains("hello-log") {
          break;
        }
      }
      assert!(Instant::now() < deadline, "log file never got output");
      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // The SIGCHLD waiter isn't running in unit tests, so the proc never
    // transitions to Exited on its own; remove it explicitly to unblock quit.
    let id = resolve(&pc, "/logged").await;
    pc.send(KernelCommand::RemoveTask(id));
    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(2), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();

    let _ = std::fs::remove_file(&log_path);
  }

  #[tokio::test]
  async fn log_path_is_resolved_with_real_pid() {
    use std::sync::{Arc, Mutex};

    let nanos = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    let mut dir = std::env::temp_dir();
    dir.push(format!("mprocs_pidlog_{}_{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();

    let kernel = Kernel::new();
    let pc = kernel.context();

    let spec = ProcessSpec::from_argv(vec![
      "sh".to_string(),
      "-c".to_string(),
      "printf hi".to_string(),
    ]);
    let seen_pid = Arc::new(Mutex::new(None::<u32>));
    let cap = seen_pid.clone();
    let log_dir = dir.clone();
    spawn_proc_task(
      &pc,
      Some(TaskPath::new("/pidlog").unwrap()),
      ProcTaskConfig {
        log: Some(Box::new(move |pid| {
          *cap.lock().unwrap() = Some(pid);
          Some(LogSink {
            path: log_dir.join(format!("{pid}.log")),
            append: false,
          })
        })),
        ..ProcTaskConfig::new(spec)
      },
    );

    let kernel_task = tokio::spawn(kernel.run());

    let deadline = Instant::now() + Duration::from_secs(2);
    let pid = loop {
      if let Some(pid) = *seen_pid.lock().unwrap() {
        let log = dir.join(format!("{pid}.log"));
        if std::fs::read_to_string(&log).is_ok_and(|c| c.contains("hi")) {
          break pid;
        }
      }
      assert!(Instant::now() < deadline, "pid-named log never got output");
      tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_ne!(pid, 0, "resolver should receive a real pid");

    let id = resolve(&pc, "/pidlog").await;
    pc.send(KernelCommand::RemoveTask(id));
    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(2), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[tokio::test]
  async fn stop_signal_cmd_runs_shell_command() {
    let nanos = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    let mut marker = std::env::temp_dir();
    marker.push(format!("mprocs_stopcmd_{}_{}", std::process::id(), nanos));

    let kernel = Kernel::new();
    let pc = kernel.context();

    let path = TaskPath::new("/sleeper").unwrap();
    let spec = ProcessSpec::from_argv(vec![
      "sh".to_string(),
      "-c".to_string(),
      "sleep 100".to_string(),
    ]);
    spawn_proc_task(
      &pc,
      Some(path),
      ProcTaskConfig {
        stop: StopSignal::Cmd(format!("printf done > {}", marker.display())),
        ..ProcTaskConfig::new(spec)
      },
    );

    let kernel_task = tokio::spawn(kernel.run());

    let id = resolve(&pc, "/sleeper").await;
    pc.send(KernelCommand::TaskCmd(id, TaskCmd::Stop));

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
      if marker.exists() {
        break;
      }
      assert!(Instant::now() < deadline, "stop command never ran");
      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    pc.send(KernelCommand::TaskCmd(id, TaskCmd::Kill));
    pc.send(KernelCommand::RemoveTask(id));
    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(2), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();

    let _ = std::fs::remove_file(&marker);
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
