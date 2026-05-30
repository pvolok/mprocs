use crate::kernel::kernel_message::SharedVt;
use crate::kernel::task::{TaskId, TaskStatus};

pub struct ConsoleState {
  pub tasks: Vec<ConsoleTaskEntry>,
  pub selected: usize,

  pub quit_modal: bool,
}

impl ConsoleState {
  pub fn clamp_selection(&mut self) {
    if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
      self.selected = self.tasks.len() - 1;
    }
  }
}

pub struct ConsoleTaskEntry {
  pub id: TaskId,
  pub path: String,
  pub status: TaskStatus,
  pub vt: Option<SharedVt>,
}
