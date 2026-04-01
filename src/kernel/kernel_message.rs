use std::{
  any::Any,
  fmt::Debug,
  ops::Deref,
  sync::{atomic::AtomicUsize, Arc, RwLock},
};

use tokio::sync::mpsc::UnboundedSender;

use crate::term::Parser;

use super::task::{TaskCmd, TaskId, TaskInit};

pub struct KernelMessage {
  pub from: TaskId,
  pub command: KernelCommand,
}

pub enum KernelCommand {
  Quit,

  AddTask(TaskId, Box<dyn FnOnce(TaskContext) -> TaskInit + Send>),
  TaskCmd(TaskId, TaskCmd),

  ListenTaskUpdates,
  UnlistenTaskUpdates,

  // Task reporting
  TaskStarted,
  TaskStopped(u32),
  TaskUpdatedScreen(Option<SharedVt>),
  TaskRendered,
}

#[derive(Clone)]
pub struct SharedVt(Arc<RwLock<Parser>>);

impl SharedVt {
  pub fn new(parser: Parser) -> Self {
    SharedVt(Arc::new(RwLock::new(parser)))
  }
}

impl Debug for SharedVt {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("SharedVt").finish()
  }
}

impl Deref for SharedVt {
  type Target = Arc<RwLock<Parser>>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[derive(Clone)]
pub struct TaskContext {
  next_task_id: Arc<AtomicUsize>,
  sender: UnboundedSender<KernelMessage>,
  pub task_id: TaskId,
}

impl TaskContext {
  pub fn new(
    next_task_id: Arc<AtomicUsize>,
    task_id: TaskId,
    sender: UnboundedSender<KernelMessage>,
  ) -> Self {
    Self {
      next_task_id,
      sender,
      task_id,
    }
  }

  pub fn send(&self, command: KernelCommand) {
    if let Err(_err) = self.sender.send(KernelMessage {
      from: self.task_id,
      command,
    }) {
      log::info!(
        "Failed to send kernel message (task_id: {}). Channel is closed.",
        self.task_id.0,
      );
    }
  }

  pub fn send_self_custom<T: Any + Send + 'static>(&self, custom: T) {
    self.send(KernelCommand::TaskCmd(
      self.task_id,
      TaskCmd::msg(custom),
    ));
  }

  pub fn alloc_id(&self) -> TaskId {
    TaskId(
      self
        .next_task_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    )
  }

  pub fn add_task(
    &self,
    f: Box<dyn FnOnce(TaskContext) -> TaskInit + Send>,
  ) -> TaskId {
    let task_id = self.alloc_id();
    self.add_task_with_id(task_id, f)
  }

  pub fn add_task_with_id(
    &self,
    task_id: TaskId,
    f: Box<dyn FnOnce(TaskContext) -> TaskInit + Send>,
  ) -> TaskId {
    self.send(KernelCommand::AddTask(task_id, f));
    task_id
  }

  pub fn get_task_sender(&self, target_id: TaskId) -> TaskSender {
    TaskSender {
      task_id: target_id,
      from_id: self.task_id,
      sender: self.sender.clone(),
    }
  }
}

#[derive(Clone)]
pub struct TaskSender {
  pub task_id: TaskId,
  pub from_id: TaskId,
  sender: UnboundedSender<KernelMessage>,
}

impl TaskSender {
  pub fn send(&self, cmd: TaskCmd) {
    let r = self.sender.send(KernelMessage {
      from: self.from_id,
      command: KernelCommand::TaskCmd(self.task_id, cmd),
    });
    if let Err(_err) = r {
      log::debug!(
        "TaskSender.send() to closed channel. from_id:{} task_id:{}",
        self.from_id.0,
        self.task_id.0
      );
    }
  }
}
