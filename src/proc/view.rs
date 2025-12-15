use crate::{
  config::ProcConfig,
  kernel::{kernel_message::SharedVt, proc::ProcId},
};

use super::{CopyMode, ReplySender};

use std::time::Instant;

/// Amount of time a process has to stay up for autorestart to trigger
pub const RESTART_THRESHOLD_SECONDS: f64 = 1.0;

#[derive(Clone, Copy)]
pub enum TargetState {
  None,
  Started,
  Stopped,
}

pub struct ProcView {
  pub id: ProcId,
  pub cfg: ProcConfig,

  pub is_up: bool,
  pub exit_code: Option<u32>,
  pub is_waiting: bool,
  pub vt: Option<SharedVt>,
  pub copy_mode: CopyMode,

  pub target_state: TargetState,
  pub last_start: Option<Instant>,
  pub changed: bool,
}

impl ProcView {
  pub fn new(id: ProcId, cfg: ProcConfig) -> Self {
    Self {
      id,
      cfg,

      is_up: false,
      exit_code: None,
      is_waiting: false,
      vt: None,
      copy_mode: CopyMode::None(None),

      target_state: TargetState::None,
      last_start: None,
      changed: false,
    }
  }

  pub fn rename(&mut self, name: &str) {
    self.cfg.name.replace_range(.., name);
  }

  pub fn id(&self) -> ProcId {
    self.id
  }

  pub fn exit_code(&self) -> Option<u32> {
    self.exit_code
  }

  pub fn lock_view(&self) -> ProcViewFrame {
    match &self.vt {
      None => ProcViewFrame::Empty,
      Some(vt) => vt
        .read()
        .map_or(ProcViewFrame::Empty, |vt| ProcViewFrame::Vt(vt)),
    }
  }

  pub fn name(&self) -> &str {
    &self.cfg.name
  }

  pub fn is_up(&self) -> bool {
    self.is_up
  }

  pub fn changed(&self) -> bool {
    self.changed
  }

  pub fn copy_mode(&self) -> &CopyMode {
    &self.copy_mode
  }

  pub fn focus(&mut self) {
    self.changed = false;
  }
}

pub enum ProcViewFrame<'a> {
  Empty,
  Vt(std::sync::RwLockReadGuard<'a, crate::vt100::Parser<ReplySender>>),
  Err(&'a str),
}
