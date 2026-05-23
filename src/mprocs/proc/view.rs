use crate::kernel::{
  kernel_message::SharedVt,
  task::{TaskId, TaskStatus},
};
use crate::mprocs::config::ProcConfig;

use super::CopyMode;

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
  pub id: TaskId,
  pub cfg: ProcConfig,

  pub status: TaskStatus,
  pub vt: SharedVt,
  pub copy_mode: CopyMode,

  pub target_state: TargetState,
  pub last_start: Option<Instant>,
  pub changed: bool,
}

impl ProcView {
  pub fn new(id: TaskId, cfg: ProcConfig, vt: SharedVt) -> Self {
    Self {
      id,
      cfg,

      status: TaskStatus::NotStarted,
      vt,
      copy_mode: CopyMode::None(None),

      target_state: TargetState::None,
      last_start: None,
      changed: false,
    }
  }

  pub fn rename(&mut self, name: &str) {
    self.cfg.name.replace_range(.., name);
  }

  pub fn id(&self) -> TaskId {
    self.id
  }

  pub fn exit_code(&self) -> Option<u32> {
    match self.status {
      TaskStatus::NotStarted => None,
      TaskStatus::Running => None,
      TaskStatus::Exited(code) => Some(code),
    }
  }

  pub fn lock_view(&'_ self) -> ProcViewFrame<'_> {
    self
      .vt
      .read()
      .map_or(ProcViewFrame::Empty, |vt| ProcViewFrame::Vt(vt))
  }

  pub fn name(&self) -> &str {
    &self.cfg.name
  }

  pub fn is_up(&self) -> bool {
    match self.status {
      TaskStatus::NotStarted => false,
      TaskStatus::Running => true,
      TaskStatus::Exited(_) => false,
    }
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
  Vt(std::sync::RwLockReadGuard<'a, crate::term::Parser>),
}
