use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::{error::ResultLogger, proc::msg::ProcCmd};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TaskId(pub usize);

pub struct TaskHandle {
  #[allow(dead_code)]
  pub task_id: TaskId,
  pub sender: UnboundedSender<ProcCmd>,

  pub stop_on_quit: bool,
  pub status: TaskStatus,

  pub deps: HashMap<TaskId, DepInfo>,
}

impl TaskHandle {
  pub fn send(&self, cmd: ProcCmd) {
    self.sender.send(cmd).log_ignore();
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskStatus {
  Down,
  Running,
}

pub struct TaskInit {
  pub sender: UnboundedSender<ProcCmd>,
  pub stop_on_quit: bool,
  pub status: TaskStatus,
  pub deps: Vec<TaskId>,
}

pub struct DepInfo {
  pub status: TaskStatus,
}
