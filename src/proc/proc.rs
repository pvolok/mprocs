use std::fmt::Debug;
use std::future::pending;
use std::path::PathBuf;

use assert_matches::assert_matches;
use crossterm::event::MouseEventKind;
use tokio::io::AsyncWriteExt;
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tui::layout::Rect;

use crate::config::ProcConfig;
use crate::encode_term::{encode_key, encode_mouse_event, KeyCodeEncodeModes};
use crate::error::ResultLogger;
use crate::kernel::kernel_message::{KernelCommand, ProcContext};
use crate::kernel::proc::{ProcId, ProcInit, ProcStatus};
use crate::key::Key;
use crate::mouse::MouseEvent;
use crate::process::process::Process as _;
use crate::process::process_spec::ProcessSpec;
use crate::vt100::{self};

use super::inst::Inst;
use super::msg::{ProcCmd, ProcEvent};
use super::view::ProcView;
use super::StopSignal;
use super::{ReplySender, Size};

fn sanitize_log_filename(name: &str) -> String {
  let mut out = String::new();
  for ch in name.chars() {
    let is_safe = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.');
    if is_safe {
      out.push(ch);
    } else if ch == ' ' {
      out.push('_');
    } else {
      out.push('_');
    }
  }

  let trimmed = out.trim_matches(|c| c == '.' || c == ' ').to_string();
  if trimmed.is_empty() {
    "process".to_string()
  } else {
    trimmed
  }
}

pub struct Proc {
  pub id: ProcId,
  pub spec: ProcessSpec,
  size: Size,

  name: String,
  stop_signal: StopSignal,
  scrollback_len: usize,
  log_dir: Option<PathBuf>,

  pub tx: UnboundedSender<ProcEvent>,

  pub inst: ProcState,
}

#[derive(Debug)]
pub enum ProcState {
  None,
  Some(Inst),
  Error(String),
}

pub fn launch_proc(
  parent_ks: &ProcContext,
  cfg: ProcConfig,
  proc_id: ProcId,
  deps: Vec<ProcId>,
  size: Rect,
) -> ProcView {
  let cfg_ = cfg.clone();
  let child_id = parent_ks.add_proc_with_id(
    proc_id,
    Box::new(move |ks| {
      let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

      let cfg = cfg_;
      tokio::spawn(async move {
        let proc_id = ks.proc_id;
        proc_main_loop(ks, proc_id, &cfg, size, cmd_receiver).await;
      });

      ProcInit {
        sender: cmd_sender,
        stop_on_quit: true,
        status: ProcStatus::Down,
        deps,
      }
    }),
  );

  ProcView::new(child_id, cfg)
}

async fn proc_main_loop(
  ks: ProcContext,
  proc_id: ProcId,
  cfg: &ProcConfig,
  size: Rect,
  mut cmd_receiver: UnboundedReceiver<ProcCmd>,
) -> ProcView {
  let (internal_sender, mut internal_receiver) =
    tokio::sync::mpsc::unbounded_channel();
  let mut proc = Proc::new(proc_id, cfg, internal_sender, size).await;
  loop {
    enum NextValue {
      Cmd(Option<ProcCmd>),
      Internal(Option<ProcEvent>),
      Read(std::io::Result<usize>),
    }
    let mut read_buf = [0u8; 128];
    let value = select! {
      cmd = cmd_receiver.recv() => NextValue::Cmd(cmd),
      event = internal_receiver.recv() => NextValue::Internal(event),
      count = proc.read(&mut read_buf) => NextValue::Read(count),
    };
    match value {
      NextValue::Cmd(Some(cmd)) => {
        let mut rendered = false;
        proc.handle_cmd(cmd, &mut rendered).await;
        if rendered {
          ks.send(KernelCommand::ProcRendered);
        }
      }
      NextValue::Cmd(None) => (),
      NextValue::Internal(Some(proc_event)) => match proc_event {
        ProcEvent::Exited(exit_code) => {
          proc.handle_exited(exit_code);
          if !proc.is_up() {
            ks.send(KernelCommand::ProcStopped(exit_code));
          }
        }
        ProcEvent::Started => {
          ks.send(KernelCommand::ProcStarted);
        }
        ProcEvent::TermReply(s) => match &mut proc.inst {
          ProcState::None => (),
          ProcState::Some(inst) => {
            inst.process.write_all(s.as_bytes()).await.log_ignore();
          }
          ProcState::Error(_) => (),
        },
        ProcEvent::SetVt(vt) => {
          ks.send(KernelCommand::ProcUpdatedScreen(vt));
        }
      },
      NextValue::Internal(None) => (),
      NextValue::Read(Ok(count)) => {
        let inst = match &mut proc.inst {
          ProcState::Some(inst) => inst,
          ProcState::None | ProcState::Error(_) => {
            log::error!("Expected proc.inst to be Some after a read.");
            continue;
          }
        };
        if count == 0 {
          inst.stdout_eof = true;
          if !proc.is_up() {
            ks.send(KernelCommand::ProcStopped(
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

          if let Ok(mut vt) = inst.vt.write() {
            vt.process(bytes);
            ks.send(KernelCommand::ProcRendered);
          }
        }
      }
      NextValue::Read(Err(e)) => {
        log::error!("Process read() error: {}", e);
        match &mut proc.inst {
          ProcState::Some(inst) => {
            inst.stdout_eof = true;
            if !proc.is_up() {
              ks.send(KernelCommand::ProcStopped(
                proc.exit_code().unwrap_or(198),
              ));
            }
          }
          ProcState::None | ProcState::Error(_) => {}
        };
      }
    }
  }
}

impl Proc {
  pub async fn new(
    id: ProcId,
    cfg: &ProcConfig,
    tx: UnboundedSender<ProcEvent>,
    size: Rect,
  ) -> Self {
    let size = Size::new(size);
    let mut proc = Proc {
      id,
      spec: cfg.into(),
      size,

      name: cfg.name.clone(),
      stop_signal: cfg.stop.clone(),
      scrollback_len: cfg.scrollback_len,
      log_dir: cfg.log_dir.clone(),

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

    let log_file = self.log_dir.as_ref().map(|dir| {
      let filename = sanitize_log_filename(&self.name);
      dir.join(format!("{}.log", filename))
    });

    let spawned = Inst::spawn(
      self.id,
      &self.spec,
      self.tx.clone(),
      &self.size,
      self.scrollback_len,
      log_file,
    )
    .await;
    let inst = match spawned {
      Ok(inst) => ProcState::Some(inst),
      Err(err) => ProcState::Error(err.to_string()),
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
      }
      ProcState::Error(_) => (),
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
      ProcState::None | ProcState::Error(_) => None,
    }
  }

  pub fn lock_vt(
    &self,
  ) -> Option<std::sync::RwLockReadGuard<'_, vt100::Parser<ReplySender>>> {
    match &self.inst {
      ProcState::None => None,
      ProcState::Some(inst) => inst.vt.read().ok(),
      ProcState::Error(_) => None,
    }
  }

  pub fn lock_vt_mut(
    &mut self,
  ) -> Option<std::sync::RwLockWriteGuard<'_, vt100::Parser<ReplySender>>> {
    match &self.inst {
      ProcState::None => None,
      ProcState::Some(inst) => inst.vt.write().ok(),
      ProcState::Error(_) => None,
    }
  }

  pub fn kill(&mut self) {
    if self.is_up() {
      if let ProcState::Some(inst) = &mut self.inst {
        let _result = inst.process.kill();
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
      StopSignal::HardKill => self.kill(),
    }
  }

  #[cfg(windows)]
  pub fn stop(&mut self) {
    match self.stop_signal.clone() {
      StopSignal::SIGINT => log::warn!("SIGINT signal is ignored on Windows"),
      StopSignal::SIGTERM => self.kill(),
      StopSignal::SIGKILL => self.kill(),
      StopSignal::SendKeys(keys) => {
        for key in keys {
          self.send_key(&key);
        }
      }
      StopSignal::HardKill => self.kill(),
    }
  }

  #[cfg(not(windows))]
  fn send_signal(&mut self, sig: libc::c_int) {
    if let ProcState::Some(inst) = &self.inst {
      unsafe { libc::kill(inst.pid as i32, sig) };
    }
  }

  pub fn resize(&mut self, size: Size) {
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
          log::warn!("Failed to encode key: {}", key.to_string());
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

  pub fn scroll_up_lines(&mut self, n: usize) {
    if let Some(mut vt) = self.lock_vt_mut() {
      vt.screen.scroll_screen_up(n);
    }
  }

  pub fn scroll_down_lines(&mut self, n: usize) {
    if let Some(mut vt) = self.lock_vt_mut() {
      vt.screen.scroll_screen_down(n);
    }
  }

  pub fn scroll_half_screen_up(&mut self) {
    self.scroll_up_lines(self.size.height as usize / 2);
  }

  pub fn scroll_half_screen_down(&mut self) {
    self.scroll_down_lines(self.size.height as usize / 2);
  }

  pub async fn handle_mouse(&mut self, event: MouseEvent) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mouse_mode = inst.vt.read().unwrap().screen().mouse_protocol_mode();
      let seq = match mouse_mode {
        vt100::MouseProtocolMode::None => String::new(),
        vt100::MouseProtocolMode::Press => match event.kind {
          MouseEventKind::Down(_)
          | MouseEventKind::ScrollDown
          | MouseEventKind::ScrollUp
          | MouseEventKind::ScrollLeft
          | MouseEventKind::ScrollRight => encode_mouse_event(event),
          _ => String::new(),
        },
        vt100::MouseProtocolMode::PressRelease => match event.kind {
          MouseEventKind::Down(_)
          | MouseEventKind::Up(_)
          | MouseEventKind::ScrollDown
          | MouseEventKind::ScrollUp
          | MouseEventKind::ScrollLeft
          | MouseEventKind::ScrollRight => encode_mouse_event(event),
          MouseEventKind::Drag(_) | MouseEventKind::Moved => String::new(),
        },
        vt100::MouseProtocolMode::ButtonMotion => match event.kind {
          MouseEventKind::Down(_)
          | MouseEventKind::Up(_)
          | MouseEventKind::ScrollDown
          | MouseEventKind::Drag(_)
          | MouseEventKind::ScrollUp
          | MouseEventKind::ScrollLeft
          | MouseEventKind::ScrollRight => encode_mouse_event(event),
          MouseEventKind::Moved => String::new(),
        },
        vt100::MouseProtocolMode::AnyMotion => encode_mouse_event(event),
      };
      let _r = inst.process.write_all(seq.as_bytes()).await;
    }
  }
}

impl Proc {
  pub async fn handle_cmd(&mut self, cmd: ProcCmd, rendered: &mut bool) {
    match cmd {
      ProcCmd::Start => {
        self.start().await;
        *rendered = true;
      }
      ProcCmd::Stop => self.stop().await,
      ProcCmd::Kill => self.kill(),

      ProcCmd::SendKey(key) => self.send_key(&key).await,
      ProcCmd::SendMouse(event) => self.handle_mouse(event).await,

      ProcCmd::ScrollUp => {
        self.scroll_half_screen_up();
        *rendered = true;
      }
      ProcCmd::ScrollDown => {
        self.scroll_half_screen_down();
        *rendered = true;
      }
      ProcCmd::ScrollUpLines { n } => {
        self.scroll_up_lines(n);
        *rendered = true;
      }
      ProcCmd::ScrollDownLines { n } => {
        self.scroll_down_lines(n);
        *rendered = true;
      }

      ProcCmd::Resize { w, h } => {
        self.resize(Size {
          width: w,
          height: h,
        });
        *rendered = true;
      }

      ProcCmd::Custom(custom) => {
        log::error!("Proc received unknown custom command: {:?}", custom);
      }

      ProcCmd::OnProcUpdate(_, _) => {
        log::warn!("Proc received ProcCmd::OnProcUpdate.");
      }
    }
  }
}
