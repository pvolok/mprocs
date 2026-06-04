use std::fmt::Debug;
use std::future::pending;

use assert_matches::assert_matches;
use tokio::io::AsyncWriteExt;
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::error::ResultLogger;
use crate::kernel::kernel_message::{KernelCommand, SharedVt, TaskContext};
use crate::kernel::task::{TaskCmd, TaskDef, TaskId};
use crate::kernel::task_path::TaskPath;
use crate::kernel::task_screen::{TaskScreen, TaskScreenCmd, TaskScreenEffect};
use crate::mprocs::config::ProcConfig;
use crate::mprocs::proc_log_config::LogConfig;
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::term::Parser;
use crate::term::encode::{KeyCodeEncodeModes, encode_key};
use crate::term::grid::Rect;
use crate::term::key::Key;

use super::Size;
use super::StopSignal;
use super::inst::Inst;
use super::msg::{ProcEvent, ProcMsg};
use super::view::ProcView;

pub struct Proc {
  pub id: TaskId,
  pub spec: ProcessSpec,
  size: Size,

  name: String,
  stop_signal: StopSignal,
  log: Option<LogConfig>,

  pub vt: SharedVt,

  pub tx: UnboundedSender<ProcEvent>,

  pub inst: ProcState,
}

#[derive(Debug)]
pub enum ProcState {
  None,
  Some(Inst),
}

pub fn launch_proc(
  parent_ks: &TaskContext,
  cfg: ProcConfig,
  task_id: TaskId,
  deps: Vec<TaskId>,
  path: Option<TaskPath>,
  size: Rect,
) -> ProcView {
  let vt =
    SharedVt::new(Parser::new(size.height, size.width, cfg.scrollback_len));
  let autostart = cfg.autostart;
  let mut cfg_ = cfg.clone();
  cfg_.autostart = false;
  let task_vt = vt.clone();
  let child_id = parent_ks.spawn_async_with_id(
    task_id,
    TaskDef {
      stop_on_quit: true,
      deps,
      path,
      vt: Some(vt.clone()),
      ..Default::default()
    },
    move |ks, cmd_receiver| async move {
      let cfg = cfg_;
      let task_id = ks.task_id;
      proc_main_loop(ks, task_id, &cfg, size, task_vt, cmd_receiver).await;
    },
  );
  if autostart {
    parent_ks.send(KernelCommand::TaskCmd(child_id, TaskCmd::Start));
  }

  ProcView::new(child_id, cfg, vt)
}

async fn proc_main_loop(
  ks: TaskContext,
  task_id: TaskId,
  cfg: &ProcConfig,
  size: Rect,
  vt: SharedVt,
  mut cmd_receiver: UnboundedReceiver<TaskCmd>,
) -> ProcView {
  let (internal_sender, mut internal_receiver) =
    tokio::sync::mpsc::unbounded_channel();
  let mut proc =
    Proc::new(task_id, cfg, vt.clone(), internal_sender, size).await;

  let mut task_screen = TaskScreen::new(task_id, vt);
  let mut screen_effects: Vec<TaskScreenEffect> = Vec::new();

  loop {
    enum NextValue {
      Cmd(Option<TaskCmd>),
      Internal(Option<ProcEvent>),
      Read(std::io::Result<usize>),
    }
    let mut read_buf = [0u8; 8 * 1024];
    let value = select! {
      cmd = cmd_receiver.recv() => NextValue::Cmd(cmd),
      event = internal_receiver.recv() => NextValue::Internal(event),
      count = proc.read(&mut read_buf) => NextValue::Read(count),
    };
    match value {
      NextValue::Cmd(Some(cmd)) => match cmd {
        TaskCmd::Start => proc.start().await,
        TaskCmd::Stop => proc.stop().await,
        TaskCmd::Kill => proc.kill().await,
        TaskCmd::Msg(msg) => {
          let msg = match msg.downcast::<ProcMsg>() {
            Ok(proc_msg) => {
              proc.handle_msg(*proc_msg).await;
              continue;
            }
            Err(msg) => msg,
          };
          let msg = match msg.downcast::<TaskScreenCmd>() {
            Ok(cmd) => {
              task_screen.handle_cmd(*cmd, &mut screen_effects);
              apply_screen_effects(&mut screen_effects, &mut proc).await;
              continue;
            }
            Err(msg) => msg,
          };
          let _ = msg;
          log::error!("Proc received unknown Msg");
        }
      },
      NextValue::Cmd(None) => (),
      NextValue::Internal(Some(proc_event)) => match proc_event {
        ProcEvent::Exited(exit_code) => {
          proc.handle_exited(exit_code);
          if !proc.is_up() {
            ks.send(KernelCommand::TaskStopped(exit_code));
          }
        }
        ProcEvent::Started => {
          ks.send(KernelCommand::TaskStarted);
        }
      },
      NextValue::Internal(None) => (),
      NextValue::Read(Ok(count)) => {
        let inst = match &mut proc.inst {
          ProcState::Some(inst) => inst,
          ProcState::None => {
            log::error!("Expected proc.inst to be Some after a read.");
            continue;
          }
        };
        if count == 0 {
          inst.stdout_eof = true;
          if !proc.is_up() {
            ks.send(KernelCommand::TaskStopped(
              proc.exit_code().unwrap_or(199),
            ));
          }
        } else {
          let bytes = &read_buf[..count];

          // Write to log file if configured
          if let Some(ref mut writer) = inst.log_writer {
            writer.write_all(bytes).await.log_ignore();
            writer.flush().await.log_ignore();
          }

          task_screen.process(bytes, &mut screen_effects);
          apply_screen_effects(&mut screen_effects, &mut proc).await;
        }
      }
      NextValue::Read(Err(e)) => {
        log::warn!("Process read() error: {}", e);
        match &mut proc.inst {
          ProcState::Some(inst) => {
            inst.stdout_eof = true;
            if !proc.is_up() {
              ks.send(KernelCommand::TaskStopped(
                proc.exit_code().unwrap_or(198),
              ));
            }
          }
          ProcState::None => {}
        };
      }
    }
  }
}

async fn apply_screen_effects(
  effects: &mut Vec<TaskScreenEffect>,
  proc: &mut Proc,
) {
  for fx in effects.drain(..) {
    match fx {
      TaskScreenEffect::Write(s) => {
        if let ProcState::Some(inst) = &mut proc.inst {
          inst.process.write_all(s.as_bytes()).await.log_ignore();
        }
      }
      TaskScreenEffect::Resize(ws) => {
        proc.resize(Size {
          width: ws.x,
          height: ws.y,
        });
      }
    }
  }
}

impl Proc {
  pub async fn new(
    id: TaskId,
    cfg: &ProcConfig,
    vt: SharedVt,
    tx: UnboundedSender<ProcEvent>,
    area: Rect,
  ) -> Self {
    let size = Size {
      width: area.width,
      height: area.height,
    };
    let mut proc = Proc {
      id,
      spec: cfg.into(),
      size,

      name: cfg.name.clone(),
      stop_signal: cfg.stop.clone(),
      log: cfg.log.clone(),

      vt,

      tx,

      inst: ProcState::None,
    };

    if cfg.autostart {
      proc.spawn_new_inst().await;
    }

    proc
  }

  async fn spawn_new_inst(&mut self) {
    assert_matches!(self.inst, ProcState::None);

    if let Ok(mut vt) = self.vt.write() {
      vt.reset();
      vt.set_size(self.size.height, self.size.width);
    }

    let spawned = Inst::spawn(
      self.id,
      &self.name,
      &self.spec,
      self.tx.clone(),
      &self.size,
      self.log.as_ref(),
    )
    .await;
    let inst = match spawned {
      Ok(inst) => ProcState::Some(inst),
      Err(err) => {
        log::warn!("Process spawn error: {}", err);
        ProcState::None
      }
    };
    self.inst = inst;
  }

  pub async fn start(&mut self) {
    if !self.is_up() {
      self.inst = ProcState::None;
      self.spawn_new_inst().await;
    }
  }

  pub fn handle_exited(&mut self, exit_code: u32) {
    match &mut self.inst {
      ProcState::None => (),
      ProcState::Some(inst) => {
        inst.exit_code = Some(exit_code);
        inst.process.on_exited();
      }
    }
  }

  pub fn is_up(&self) -> bool {
    if let ProcState::Some(inst) = &self.inst {
      inst.exit_code.is_none() || !inst.stdout_eof
    } else {
      false
    }
  }

  pub fn exit_code(&self) -> Option<u32> {
    match &self.inst {
      ProcState::Some(inst) => inst.exit_code,
      ProcState::None => None,
    }
  }

  pub fn lock_vt(&self) -> Option<std::sync::RwLockReadGuard<'_, Parser>> {
    self.vt.read().ok()
  }

  pub fn lock_vt_mut(
    &mut self,
  ) -> Option<std::sync::RwLockWriteGuard<'_, Parser>> {
    self.vt.write().ok()
  }

  pub async fn kill(&mut self) {
    if self.is_up() {
      if let ProcState::Some(inst) = &mut self.inst {
        inst.process.kill().await.log_ignore();
      }
    }
  }

  #[cfg(not(windows))]
  pub async fn stop(&mut self) {
    match self.stop_signal.clone() {
      StopSignal::SIGINT => self.send_signal(libc::SIGINT),
      StopSignal::SIGTERM => self.send_signal(libc::SIGTERM),
      StopSignal::SIGKILL => self.send_signal(libc::SIGKILL),
      StopSignal::SendKeys(keys) => {
        for key in keys {
          self.send_key(&key).await;
        }
      }
      StopSignal::HardKill => self.kill().await,
      StopSignal::Cmd(shell) => self.run_stop_cmd(shell),
    }
  }

  #[cfg(windows)]
  pub async fn stop(&mut self) {
    match self.stop_signal.clone() {
      StopSignal::SIGINT => log::debug!("SIGINT signal is ignored on Windows"),
      StopSignal::SIGTERM => self.kill().await,
      StopSignal::SIGKILL => self.kill().await,
      StopSignal::SendKeys(keys) => {
        for key in keys {
          self.send_key(&key).await;
        }
      }
      StopSignal::HardKill => self.kill().await,
      StopSignal::Cmd(shell) => self.run_stop_cmd(shell),
    }
  }

  /// Spawn the configured stop command as a separate subprocess. Inherits
  /// the proc's cwd and env so commands like `podman compose down` target
  /// the same project. Output is discarded. The main proc is expected to
  /// exit on its own once the command takes effect.
  fn run_stop_cmd(&self, shell: String) {
    let cwd = self.spec.cwd.clone();
    let env = self.spec.env.clone();
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
  fn send_signal(&mut self, sig: libc::c_int) {
    if let ProcState::Some(inst) = &self.inst {
      unsafe { libc::kill(inst.pid as i32, sig) };
    }
  }

  pub fn resize(&mut self, size: Size) {
    if let Ok(mut vt) = self.vt.write() {
      vt.set_size(size.height, size.width);
    }
    if let ProcState::Some(inst) = &mut self.inst {
      inst.resize(&size);
    }
    self.size = size;
  }

  pub async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    if let ProcState::Some(inst) = &mut self.inst {
      if !inst.stdout_eof {
        return inst.process.read(buf).await;
      }
    }
    pending().await
  }

  pub async fn send_key(&mut self, key: &Key) {
    if self.is_up() {
      let application_cursor_keys = self
        .lock_vt()
        .is_some_and(|vt| vt.screen().application_cursor());
      let encoder = encode_key(
        key,
        KeyCodeEncodeModes {
          enable_csi_u_key_encoding: true,
          application_cursor_keys,
          newline_mode: false,
        },
      );
      match encoder {
        Ok(encoder) => {
          self.write_all(encoder.as_bytes()).await;
        }
        Err(_) => {
          log::warn!("Failed to encode key: {}", key.spec());
        }
      }
    }
  }

  pub async fn write_all(&mut self, bytes: &[u8]) {
    if self.is_up() {
      if let Some(mut vt) = self.lock_vt_mut() {
        if vt.screen().scrollback() > 0 {
          vt.set_scrollback(0);
        }
      }
      if let ProcState::Some(inst) = &mut self.inst {
        inst.process.write_all(bytes).await.log_ignore();
      }
    }
  }
}

impl Proc {
  pub async fn handle_msg(&mut self, msg: ProcMsg) {
    match msg {
      ProcMsg::SendKey(key) => self.send_key(&key).await,
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
  };

  use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

  use super::*;
  use crate::{
    kernel::{
      kernel::Kernel,
      task::{Effects, Task},
    },
    mprocs::config::CmdConfig,
  };

  #[derive(Debug, PartialEq)]
  enum RecordedCmd {
    Start,
    Stop,
  }

  struct RecordingTask {
    tx: tokio::sync::mpsc::UnboundedSender<RecordedCmd>,
  }

  impl Task for RecordingTask {
    fn handle_cmd(&mut self, cmd: TaskCmd, fx: &mut Effects) {
      match cmd {
        TaskCmd::Start => {
          self.tx.send(RecordedCmd::Start).unwrap();
          fx.started();
        }
        TaskCmd::Stop | TaskCmd::Kill => {
          self.tx.send(RecordedCmd::Stop).unwrap();
          fx.stopped(0);
        }
        TaskCmd::Msg(_) => {}
      }
    }
  }

  fn recording_task() -> (
    UnboundedReceiver<RecordedCmd>,
    impl FnOnce(TaskContext) -> Box<dyn Task> + 'static,
  ) {
    let (tx, rx) = unbounded_channel();
    (rx, move |_| Box::new(RecordingTask { tx }))
  }

  async fn flush_kernel(pc: &TaskContext) {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    pc.send(KernelCommand::Query(
      crate::kernel::kernel_message::KernelQuery::ListTasks(None),
      response_tx,
    ));
    tokio::time::timeout(Duration::from_secs(1), response_rx)
      .await
      .expect("timed out waiting for kernel query response")
      .expect("kernel query response channel closed");
  }

  async fn recv_cmd(rx: &mut UnboundedReceiver<RecordedCmd>) -> RecordedCmd {
    tokio::time::timeout(Duration::from_secs(1), rx.recv())
      .await
      .expect("timed out waiting for task command")
      .expect("task command channel closed")
  }

  async fn assert_path_absent_for(path: &Path, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
      assert!(
        !path.exists(),
        "path unexpectedly exists: {}",
        path.display()
      );
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  }

  async fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
      if path.exists() {
        return;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for path: {}", path.display());
  }

  #[cfg(not(windows))]
  #[tokio::test]
  async fn autostart_proc_waits_for_dependencies() {
    let mut temp_dir = std::env::temp_dir();
    let nanos = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap()
      .as_nanos();
    temp_dir.push(format!(
      "mprocs_autostart_deps_{}_{}",
      std::process::id(),
      nanos
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let marker_path = temp_dir.join("started");

    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (mut provider_rx, provider_task) = recording_task();
    let provider_id = kernel.register_task(TaskDef::default(), provider_task);

    let proc_config = ProcConfig {
      name: "dependent".to_string(),
      cmd: CmdConfig::Shell {
        shell: format!("printf started > {}", marker_path.display()),
      },
      cwd: None,
      env: None,
      autostart: true,
      autorestart: false,
      stop: StopSignal::SIGKILL,
      deps: Vec::new(),
      mouse_scroll_speed: 1,
      scrollback_len: 100,
      log: None,
    };

    let proc_view = launch_proc(
      &pc,
      proc_config,
      pc.alloc_id(),
      vec![provider_id],
      None,
      Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
      },
    );

    let kernel_task = tokio::spawn(kernel.run());

    flush_kernel(&pc).await;
    assert_path_absent_for(&marker_path, Duration::from_millis(100)).await;

    pc.send(KernelCommand::TaskCmd(provider_id, TaskCmd::Start));
    assert_eq!(recv_cmd(&mut provider_rx).await, RecordedCmd::Start);

    wait_for_path(&marker_path).await;

    pc.send(KernelCommand::RemoveTask(proc_view.id()));
    flush_kernel(&pc).await;

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(2), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();

    let _ = std::fs::remove_dir_all(temp_dir);
  }
}
