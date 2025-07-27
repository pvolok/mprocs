use std::fmt::Debug;
use std::io::Write;
use std::sync::atomic::AtomicUsize;

use assert_matches::assert_matches;
use portable_pty::CommandBuilder;
use tokio::select;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tui::layout::Rect;

use crate::config::ProcConfig;
use crate::encode_term::{encode_key, encode_mouse_event, KeyCodeEncodeModes};
use crate::error::ResultLogger;
use crate::kernel2::kernel_message::{KernelCommand, KernelSender2, SharedVt};
use crate::kernel2::proc::{ProcId, ProcInit, ProcStatus};
use crate::key::Key;
use crate::mouse::MouseEvent;
use crate::vt100::{self};

use super::handle::ProcHandle;
use super::inst::Inst;
use super::msg::{ProcCmd, ProcEvent};
use super::StopSignal;
use super::{Pos, ReplySender, Size};

pub struct Proc {
  pub id: ProcId,
  pub cmd: CommandBuilder,
  size: Size,

  stop_signal: StopSignal,
  mouse_scroll_speed: usize,
  scrollback_len: usize,

  pub tx: UnboundedSender<ProcEvent>,

  pub inst: ProcState,
}

static NEXT_PROC_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub enum ProcState {
  None,
  Some(Inst),
  Error(String),
}

pub fn launch_proc(
  parent_ks: &KernelSender2,
  cfg: ProcConfig,
  size: Rect,
) -> ProcHandle {
  let cfg_ = cfg.clone();
  let child_ks = parent_ks.add_proc(Box::new(move |ks| {
    let (cmd_sender, cmd_receiver) = tokio::sync::mpsc::unbounded_channel();

    let cfg = cfg_;
    tokio::spawn(async move {
      let proc_id = ks.proc_id;
      proc_main_loop(ks, proc_id, &cfg, size, cmd_receiver).await;
    });

    ProcInit {
      sender: cmd_sender,
      stop_on_quit: true,
      status: ProcStatus::Running,
    }
  }));

  ProcHandle::new(child_ks.proc_id, cfg)
}

async fn proc_main_loop(
  ks: KernelSender2,
  proc_id: ProcId,
  cfg: &ProcConfig,
  size: Rect,
  mut cmd_receiver: UnboundedReceiver<ProcCmd>,
) -> ProcHandle {
  let (internal_sender, mut internal_receiver) =
    tokio::sync::mpsc::unbounded_channel();
  let mut proc = Proc::new(proc_id, cfg, internal_sender, size);
  loop {
    enum NextValue {
      Cmd(Option<ProcCmd>),
      Internal(Option<ProcEvent>),
    }
    let value = select! {
      cmd = cmd_receiver.recv() => NextValue::Cmd(cmd),
      event = internal_receiver.recv() => NextValue::Internal(event),
    };
    match value {
      NextValue::Cmd(Some(cmd)) => {
        let mut rendered = false;
        proc.handle_cmd(cmd, &mut rendered);
        if rendered {
          ks.send(KernelCommand::ProcRendered);
        }
      }
      NextValue::Cmd(None) => (),
      NextValue::Internal(Some(proc_event)) => match proc_event {
        ProcEvent::Render => ks.send(KernelCommand::ProcRendered),
        ProcEvent::Exited(exit_code) => {
          proc.handle_exited(exit_code);
          if !proc.is_up() {
            ks.send(KernelCommand::ProcStopped(exit_code));
          }
        }
        ProcEvent::StdoutEOF => {
          proc.handle_stdout_eof();
          if !proc.is_up() {
            ks.send(KernelCommand::ProcStopped(
              proc.exit_code().unwrap_or(199),
            ));
          }
        }
        ProcEvent::Started => {
          ks.send(KernelCommand::ProcStarted);
        }
        ProcEvent::TermReply(s) => match &mut proc.inst {
          ProcState::None => (),
          ProcState::Some(inst) => {
            let _ = inst.writer.write_all(s.as_bytes());
          }
          ProcState::Error(_) => (),
        },
        ProcEvent::SetVt(vt) => {
          ks.send(KernelCommand::ProcUpdatedScreen(vt));
        }
      },
      NextValue::Internal(None) => (),
    }
  }
}

impl Proc {
  pub fn new(
    id: ProcId,
    cfg: &ProcConfig,
    tx: UnboundedSender<ProcEvent>,
    size: Rect,
  ) -> Self {
    let size = Size::new(size);
    let mut proc = Proc {
      id,
      cmd: cfg.into(),
      size,

      stop_signal: cfg.stop.clone(),
      mouse_scroll_speed: cfg.mouse_scroll_speed,
      scrollback_len: cfg.scrollback_len,

      tx,

      inst: ProcState::None,
    };

    if cfg.autostart {
      proc.spawn_new_inst();
    }

    proc
  }

  fn spawn_new_inst(&mut self) {
    assert_matches!(self.inst, ProcState::None);

    let spawned = Inst::spawn(
      self.id,
      self.cmd.clone(),
      self.tx.clone(),
      &self.size,
      self.scrollback_len,
    );
    let inst = match spawned {
      Ok(inst) => ProcState::Some(inst),
      Err(err) => ProcState::Error(err.to_string()),
    };
    self.inst = inst;
  }

  pub fn start(&mut self) {
    if !self.is_up() {
      self.inst = ProcState::None;
      self.spawn_new_inst();
    }
  }

  pub fn handle_exited(&mut self, exit_code: u32) {
    match &mut self.inst {
      ProcState::None => (),
      ProcState::Some(inst) => {
        inst.master = None;
        inst.exit_code = Some(exit_code);
      }
      ProcState::Error(_) => (),
    }
  }

  pub fn handle_stdout_eof(&mut self) {
    match &mut self.inst {
      ProcState::None => (),
      ProcState::Some(inst) => inst.stdout_eof = true,
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

  pub fn clone_vt(&self) -> Option<SharedVt> {
    match &self.inst {
      ProcState::None => None,
      ProcState::Some(inst) => Some(inst.vt.clone()),
      ProcState::Error(_) => None,
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
        let _result = inst.killer.kill();
      }
    }
  }

  #[cfg(not(windows))]
  pub fn stop(&mut self) {
    match self.stop_signal.clone() {
      StopSignal::SIGINT => self.send_signal(libc::SIGINT),
      StopSignal::SIGTERM => self.send_signal(libc::SIGTERM),
      StopSignal::SIGKILL => self.send_signal(libc::SIGKILL),
      StopSignal::SendKeys(keys) => {
        for key in keys {
          self.send_key(&key);
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

  pub fn resize(&mut self, size: Rect) {
    let size = Size::new(size);
    if let ProcState::Some(inst) = &self.inst {
      inst.resize(&size);
    }
    self.size = size;
  }

  pub fn send_key(&mut self, key: &Key) {
    if self.is_up() {
      let application_cursor_keys = self
        .lock_vt()
        .map_or(false, |vt| vt.screen().application_cursor());
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
          self.write_all(encoder.as_bytes());
        }
        Err(_) => {
          log::warn!("Failed to encode key: {}", key.to_string());
        }
      }
    }
  }

  pub fn write_all(&mut self, bytes: &[u8]) {
    if self.is_up() {
      if let Some(mut vt) = self.lock_vt_mut() {
        if vt.screen().scrollback() > 0 {
          vt.set_scrollback(0);
        }
      }
      if let ProcState::Some(inst) = &mut self.inst {
        inst.writer.write_all(bytes).log_ignore();
      }
    }
  }

  pub fn scroll_up_lines(&mut self, n: usize) {
    if let Some(mut vt) = self.lock_vt_mut() {
      vt.screen.scroll_screen_up(n);
    }
  }

  fn scroll_vt_up(vt: &mut vt100::Parser<ReplySender>, n: usize) {
    let pos = usize::saturating_add(vt.screen().scrollback(), n);
    vt.set_scrollback(pos);
  }

  pub fn scroll_down_lines(&mut self, n: usize) {
    if let Some(mut vt) = self.lock_vt_mut() {
      vt.screen.scroll_screen_down(n);
    }
  }

  fn scroll_vt_down(vt: &mut vt100::Parser<ReplySender>, n: usize) {
    let pos = usize::saturating_sub(vt.screen().scrollback(), n);
    vt.set_scrollback(pos);
  }

  pub fn scroll_half_screen_up(&mut self) {
    self.scroll_up_lines(self.size.height as usize / 2);
  }

  pub fn scroll_half_screen_down(&mut self) {
    self.scroll_down_lines(self.size.height as usize / 2);
  }

  pub fn handle_mouse(&mut self, event: MouseEvent) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mouse_mode = inst.vt.read().unwrap().screen().mouse_protocol_mode();
      match mouse_mode {
        vt100::MouseProtocolMode::None => (),
        vt100::MouseProtocolMode::Press
        | vt100::MouseProtocolMode::PressRelease
        | vt100::MouseProtocolMode::ButtonMotion
        | vt100::MouseProtocolMode::AnyMotion => {
          let seq = encode_mouse_event(event);
          let _r = inst.writer.write_all(seq.as_bytes());
        }
      }
    }
  }
}

impl Proc {
  pub fn handle_cmd(&mut self, cmd: ProcCmd, rendered: &mut bool) {
    match cmd {
      ProcCmd::Start => {
        self.start();
        *rendered = true;
      }
      ProcCmd::Stop => self.stop(),
      ProcCmd::Kill => self.kill(),

      ProcCmd::SendKey(key) => self.send_key(&key),
      ProcCmd::SendMouse(event) => self.handle_mouse(event),

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

      ProcCmd::Resize { x, y, w, h } => {
        self.resize(Rect {
          x,
          y,
          width: w,
          height: h,
        });
        *rendered = true;
      }

      ProcCmd::OnProcUpdate(_, _) => {
        log::warn!("Proc received ProcCmd::OnProcUpdate.");
      }
    }
  }
}

fn translate_mouse_pos(event: &MouseEvent, scrollback: usize) -> Pos {
  Pos {
    y: event.y - scrollback as i32,
    x: event.x,
  }
}
