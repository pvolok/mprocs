use std::{
  any::Any,
  fmt::Debug,
  ops::Deref,
  sync::{Arc, RwLock, atomic::AtomicUsize},
};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::term::Parser;

use super::task::{Task, TaskCmd, TaskDef, TaskId, TaskStatus};
use super::task_path::TaskPath;

pub struct KernelMessage {
  pub from: TaskId,
  pub command: KernelCommand,
}

pub enum KernelCommand {
  Quit,

  RegisterTask(
    TaskId,
    TaskDef,
    Box<dyn FnOnce(TaskContext) -> Box<dyn Task> + Send>,
  ),
  RemoveTask(TaskId),
  TaskCmd(TaskId, TaskCmd),

  TaskCmdByPath(TaskPath, TaskCmd),

  SetTaskPath(TaskId, TaskPath),

  Query(
    KernelQuery,
    tokio::sync::oneshot::Sender<KernelQueryResponse>,
  ),

  ListenTaskUpdates,
  UnlistenTaskUpdates,

  // Task reporting
  TaskStarted,
  TaskStopped(u32),
  TaskUpdatedScreen(Option<SharedVt>),
  TaskRendered,
}

pub enum KernelQuery {
  /// List tasks matching an optional glob. None = list all.
  ListTasks(Option<String>),
  /// Resolve a path to a TaskId.
  ResolvePath(TaskPath),
  /// Get the current screen content for a task (rendered as ANSI text).
  GetScreen(TaskPath),
}

pub enum KernelQueryResponse {
  TaskList(Vec<TaskInfo>),
  ResolvedPath(Option<TaskId>),
  /// ANSI-rendered screen content, or None if the task has no screen.
  Screen(Option<String>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskInfo {
  pub id: TaskId,
  pub path: Option<TaskPath>,
  pub status: TaskStatus,
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
    self.send(KernelCommand::TaskCmd(self.task_id, TaskCmd::msg(custom)));
  }

  pub fn alloc_id(&self) -> TaskId {
    TaskId(
      self
        .next_task_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    )
  }

  pub fn register(
    &self,
    def: TaskDef,
    factory: Box<dyn FnOnce(TaskContext) -> Box<dyn Task> + Send>,
  ) -> TaskId {
    let task_id = self.alloc_id();
    self.register_with_id(task_id, def, factory)
  }

  pub fn register_with_id(
    &self,
    task_id: TaskId,
    def: TaskDef,
    factory: Box<dyn FnOnce(TaskContext) -> Box<dyn Task> + Send>,
  ) -> TaskId {
    self.send(KernelCommand::RegisterTask(task_id, def, factory));
    task_id
  }

  pub fn spawn_async<F, Fut>(&self, def: TaskDef, f: F) -> TaskId
  where
    F: FnOnce(TaskContext, tokio::sync::mpsc::UnboundedReceiver<TaskCmd>) -> Fut
      + Send
      + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
  {
    let task_id = self.alloc_id();
    self.spawn_async_with_id(task_id, def, f)
  }

  pub fn spawn_async_with_id<F, Fut>(
    &self,
    task_id: TaskId,
    def: TaskDef,
    f: F,
  ) -> TaskId
  where
    F: FnOnce(TaskContext, tokio::sync::mpsc::UnboundedReceiver<TaskCmd>) -> Fut
      + Send
      + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
  {
    use super::task::ChannelTask;
    self.register_with_id(
      task_id,
      def,
      Box::new(|ctx| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(f(ctx, rx));
        Box::new(ChannelTask::new(tx))
      }),
    )
  }

  pub fn send_to_path(&self, path: TaskPath, cmd: TaskCmd) {
    self.send(KernelCommand::TaskCmdByPath(path, cmd));
  }

  pub fn set_task_path(&self, task_id: TaskId, path: TaskPath) {
    self.send(KernelCommand::SetTaskPath(task_id, path));
  }

  pub fn query(
    &self,
    query: KernelQuery,
  ) -> tokio::sync::oneshot::Receiver<KernelQueryResponse> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    self.send(KernelCommand::Query(query, tx));
    rx
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
