use std::any::Any;
use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::kernel_message::SharedVt;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TaskId(pub usize);

pub trait KernelTask: Send {
  fn handle_cmd(&mut self, cmd: TaskCmd);
}

pub enum TaskCmd {
  Start,
  Stop,
  Kill,
  Notify(TaskId, TaskNotify),
  Msg(Box<dyn Any + Send>),
}

impl TaskCmd {
  pub fn msg(m: impl Any + Send + 'static) -> Self {
    TaskCmd::Msg(Box::new(m))
  }
}

impl fmt::Debug for TaskCmd {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TaskCmd::Start => write!(f, "Start"),
      TaskCmd::Stop => write!(f, "Stop"),
      TaskCmd::Kill => write!(f, "Kill"),
      TaskCmd::Notify(id, n) => write!(f, "Notify({:?}, {:?})", id, n),
      TaskCmd::Msg(_) => write!(f, "Msg(...)"),
    }
  }
}

pub enum TaskNotify {
  Started,
  Stopped(u32),
  Rendered,
  ScreenChanged(Option<SharedVt>),
}

impl fmt::Debug for TaskNotify {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TaskNotify::Started => write!(f, "Started"),
      TaskNotify::Stopped(code) => write!(f, "Stopped({})", code),
      TaskNotify::Rendered => write!(f, "Rendered"),
      TaskNotify::ScreenChanged(_) => write!(f, "ScreenChanged(...)"),
    }
  }
}

pub struct ChannelTask {
  sender: UnboundedSender<TaskCmd>,
}

impl ChannelTask {
  pub fn new(sender: UnboundedSender<TaskCmd>) -> Self {
    Self { sender }
  }
}

impl KernelTask for ChannelTask {
  fn handle_cmd(&mut self, cmd: TaskCmd) {
    let _ = self.sender.send(cmd);
  }
}

pub struct NoopTask;

impl KernelTask for NoopTask {
  fn handle_cmd(&mut self, _cmd: TaskCmd) {}
}

pub struct TaskHandle {
  #[allow(dead_code)]
  pub task_id: TaskId,
  pub task: Box<dyn KernelTask>,

  pub stop_on_quit: bool,
  pub status: TaskStatus,

  pub deps: HashMap<TaskId, DepInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskStatus {
  Down,
  Running,
}

pub struct TaskInit {
  pub task: Box<dyn KernelTask>,
  pub stop_on_quit: bool,
  pub status: TaskStatus,
  pub deps: Vec<TaskId>,
}

pub struct DepInfo {
  pub status: TaskStatus,
}
