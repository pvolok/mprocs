use crate::kernel::{
  kernel_message::SharedVt,
  task::{TaskId, TaskStatus},
};
use crate::mprocs::config::ProcConfig;

pub struct ProcView {
  pub id: TaskId,
  pub cfg: ProcConfig,

  pub status: TaskStatus,
  pub vt: SharedVt,
  /// Presentation surface from the kernel's copy mode, rendered instead of
  /// `vt` while copy mode is active. Set/cleared by `CopyEntered`/`CopyLeft`.
  pub present: Option<SharedVt>,

  pub changed: bool,
}

impl ProcView {
  pub fn new(
    id: TaskId,
    cfg: ProcConfig,
    status: TaskStatus,
    vt: SharedVt,
  ) -> Self {
    Self {
      id,
      cfg,

      status,
      vt,
      present: None,

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

  pub fn copy_active(&self) -> bool {
    self.present.is_some()
  }

  pub fn focus(&mut self) {
    self.changed = false;
  }
}

pub enum ProcViewFrame<'a> {
  Empty,
  Vt(std::sync::RwLockReadGuard<'a, crate::term::Parser>),
}
