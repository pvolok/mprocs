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
  UpdatedScreen(Option<SharedVt>),
  Rendered,
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

  pub fn updated_screen(&mut self, vt: Option<SharedVt>) {
    self.0.push(TaskEffect::UpdatedScreen(vt));
  }

  pub fn rendered(&mut self) {
    self.0.push(TaskEffect::Rendered);
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
  Added(Option<TaskPath>, TaskStatus),
  Started,
  Stopped(u32),
  Rendered,
  ScreenChanged(Option<SharedVt>),
  Removed,
}

impl fmt::Debug for TaskNotify {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TaskNotify::Added(path, status) => {
        write!(f, "Added({:?}, {:?})", path, status)
      }
      TaskNotify::Started => write!(f, "Started"),
      TaskNotify::Stopped(code) => write!(f, "Stopped({})", code),
      TaskNotify::Rendered => write!(f, "Rendered"),
      TaskNotify::ScreenChanged(_) => write!(f, "ScreenChanged(...)"),
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
  Down,
  Running,
}

pub struct DepInfo {
  pub status: TaskStatus,
}

pub struct TaskDef {
  pub stop_on_quit: bool,
  pub status: TaskStatus,
  pub deps: Vec<TaskId>,
  pub path: Option<TaskPath>,
}

impl Default for TaskDef {
  fn default() -> Self {
    Self {
      stop_on_quit: false,
      status: TaskStatus::Down,
      deps: Vec::new(),
      path: None,
    }
  }
}
