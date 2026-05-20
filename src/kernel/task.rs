use std::any::Any;
use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use super::kernel_message::SharedVt;
use super::task_path::TaskPath;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TaskId(pub usize);

pub trait Task: Send + 'static {
  fn handle_cmd(&mut self, cmd: TaskCmd, fx: &mut Effects);
}

pub enum TaskEffect {
  Started,
  Stopped(u32),
  Remove,
}

pub struct Effects(Vec<TaskEffect>);

impl Effects {
  pub fn new() -> Self {
    Self(Vec::new())
  }

  pub fn started(&mut self) {
    self.0.push(TaskEffect::Started);
  }

  pub fn stopped(&mut self, code: u32) {
    self.0.push(TaskEffect::Stopped(code));
  }

  pub fn remove(&mut self) {
    self.0.push(TaskEffect::Remove);
  }

  pub fn drain(&mut self) -> std::vec::Drain<'_, TaskEffect> {
    self.0.drain(..)
  }
}

pub enum TaskCmd {
  Start,
  Stop,
  Kill,
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
      TaskCmd::Msg(_) => write!(f, "Msg(...)"),
    }
  }
}

pub struct TaskNotification {
  pub from: TaskId,
  pub notify: TaskNotify,
}

#[derive(Clone)]
pub enum TaskNotify {
  Added(Option<TaskPath>, TaskStatus, Option<SharedVt>),
  Started,
  Stopped(u32),
  Removed,
}

impl fmt::Debug for TaskNotify {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TaskNotify::Added(path, status, _) => {
        write!(f, "Added({:?}, {:?})", path, status)
      }
      TaskNotify::Started => write!(f, "Started"),
      TaskNotify::Stopped(code) => write!(f, "Stopped({})", code),
      TaskNotify::Removed => write!(f, "Removed"),
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

impl Task for ChannelTask {
  fn handle_cmd(&mut self, cmd: TaskCmd, _fx: &mut Effects) {
    let _ = self.sender.send(cmd);
  }
}

pub struct TaskHandle {
  #[allow(dead_code)]
  pub task_id: TaskId,
  pub task: Box<dyn Task>,

  pub stop_on_quit: bool,
  pub status: TaskStatus,

  pub deps: HashMap<TaskId, DepInfo>,

  pub path: Option<TaskPath>,
  pub vt: Option<SharedVt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
  NotStarted,
  Running,
  Exited(u32),
}

pub struct DepInfo {
  pub status: TaskStatus,
}

pub struct TaskDef {
  pub stop_on_quit: bool,
  pub status: TaskStatus,
  pub deps: Vec<TaskId>,
  pub path: Option<TaskPath>,
  pub vt: Option<SharedVt>,
}

impl Default for TaskDef {
  fn default() -> Self {
    Self {
      stop_on_quit: false,
      status: TaskStatus::NotStarted,
      deps: Vec::new(),
      path: None,
      vt: None,
    }
  }
}
