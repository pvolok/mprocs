use crate::kernel::{
  kernel_message::SharedVt,
  task::{TaskId, TaskStatus},
};

pub struct ProcView {
  pub id: TaskId,
  pub name: String,

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
    name: String,
    status: TaskStatus,
    vt: SharedVt,
  ) -> Self {
    Self {
      id,
      name,
      status,
      vt,
      present: None,
      changed: false,
    }
  }

  pub fn set_name(&mut self, name: String) {
    self.name = name;
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

  pub fn name(&self) -> &str {
    &self.name
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
