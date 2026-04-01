use std::{
  collections::{HashMap, HashSet},
  sync::{atomic::AtomicUsize, Arc},
};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{error::ResultLogger, kernel::kernel_message::TaskContext};

use super::{
  kernel_message::{KernelCommand, KernelMessage},
  task::{
    DepInfo, TaskCmd, TaskHandle, TaskId, TaskInit, TaskNotify, TaskStatus,
  },
};

pub struct Kernel {
  sender: UnboundedSender<KernelMessage>,
  receiver: UnboundedReceiver<KernelMessage>,

  quitting: bool,
  next_task_id: Arc<AtomicUsize>,
  tasks: HashMap<TaskId, TaskHandle>,
  /// If `a` requires `b`, then `rev_deps = {b: [a]}`.
  rev_deps: HashMap<TaskId, HashSet<TaskId>>,
  listeners: HashSet<TaskId>,
}

impl Kernel {
  pub fn new() -> Self {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    Self {
      sender,
      receiver,

      quitting: false,
      next_task_id: Arc::new(AtomicUsize::new(1)),
      tasks: HashMap::new(),
      rev_deps: HashMap::new(),
      listeners: Default::default(),
    }
  }

  pub fn spawn_task<F>(&mut self, f: F) -> TaskId
  where
    F: FnOnce(TaskContext) -> TaskInit,
  {
    let task_id = TaskId(
      self
        .next_task_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    );
    self.spawn_task_with_id(task_id, f);
    task_id
  }

  pub fn spawn_task_with_id<F>(&mut self, task_id: TaskId, f: F)
  where
    F: FnOnce(TaskContext) -> TaskInit,
  {
    let kernel_sender =
      TaskContext::new(self.next_task_id.clone(), task_id, self.sender.clone());
    let init = f(kernel_sender);
    let mut task_handle = TaskHandle {
      task_id,
      task: init.task,

      stop_on_quit: init.stop_on_quit,
      status: init.status,

      deps: HashMap::new(),
    };

    for dep_id in &init.deps {
      task_handle.deps.insert(
        *dep_id,
        DepInfo {
          status: self
            .tasks
            .get(dep_id)
            .map_or(TaskStatus::Down, |d| d.status),
        },
      );
      self.rev_deps.entry(*dep_id).or_default().insert(task_id);
    }

    self.tasks.insert(task_id, task_handle);
  }

  pub async fn run(mut self) {
    loop {
      let msg = if let Some(msg) = self.receiver.recv().await {
        msg
      } else {
        log::warn!("Kernel receiver returned None.");
        break;
      };

      match msg.command {
        KernelCommand::Quit => {
          if self.quitting {
            break;
          }
          self.quitting = true;

          let task_ids: Vec<TaskId> = self.tasks.keys().copied().collect();
          for task_id in task_ids {
            if let Some(task) = self.tasks.get_mut(&task_id) {
              task.task.handle_cmd(TaskCmd::Stop);
            }
          }

          if self.is_ready_to_quit() {
            break;
          }
        }

        KernelCommand::AddTask(task_id, create_task) => {
          self.spawn_task_with_id(task_id, create_task);
        }
        KernelCommand::TaskCmd(task_id, cmd) => {
          if let Some(task) = self.tasks.get_mut(&task_id) {
            match cmd {
              TaskCmd::Start => {
                let all_deps_ready = task
                  .deps
                  .iter()
                  .all(|(_, dep)| dep.status == TaskStatus::Running);
                if all_deps_ready {
                  task.task.handle_cmd(cmd);
                }
              }
              TaskCmd::Stop | TaskCmd::Kill => {
                task.task.handle_cmd(cmd);
              }
              _ => {
                task.task.handle_cmd(cmd);
              }
            }
          }
        }

        KernelCommand::TaskStarted => {
          // Went from DOWN to UP.
          let mut started = false;
          if let Some(task) = self.tasks.get_mut(&msg.from) {
            match task.status {
              TaskStatus::Down => {
                started = true;
              }
              TaskStatus::Running => (),
            }
            task.status = TaskStatus::Running;
          }

          if started {
            if let Some(rev_deps) = self.rev_deps.get(&msg.from) {
              for rev_dep_id in rev_deps {
                if let Some(rev_dep) = self.tasks.get_mut(rev_dep_id) {
                  let mut all_deps_ready = true;
                  for (dep_id, dep) in &mut rev_dep.deps {
                    if *dep_id == msg.from {
                      dep.status = TaskStatus::Running;
                    }
                    if dep.status != TaskStatus::Running {
                      all_deps_ready = false;
                    }
                  }
                  if all_deps_ready {
                    self
                      .sender
                      .send(KernelMessage {
                        from: TaskId(0),
                        command: KernelCommand::TaskCmd(
                          *rev_dep_id,
                          TaskCmd::Start,
                        ),
                      })
                      .log_ignore();
                  }
                }
              }
            }
          }

          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.tasks.get_mut(&listener_id) {
              listener
                .task
                .handle_cmd(TaskCmd::Notify(msg.from, TaskNotify::Started));
            }
          }
        }
        KernelCommand::TaskStopped(exit_code) => {
          if let Some(task) = self.tasks.get_mut(&msg.from) {
            task.status = TaskStatus::Down;
          }

          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.tasks.get_mut(&listener_id) {
              listener.task.handle_cmd(TaskCmd::Notify(
                msg.from,
                TaskNotify::Stopped(exit_code),
              ));
            }
          }

          if self.quitting && self.is_ready_to_quit() {
            break;
          }
        }
        KernelCommand::TaskUpdatedScreen(vt) => {
          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.tasks.get_mut(&listener_id) {
              listener.task.handle_cmd(TaskCmd::Notify(
                msg.from,
                TaskNotify::ScreenChanged(vt.clone()),
              ));
            }
          }
        }
        KernelCommand::TaskRendered => {
          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.tasks.get_mut(&listener_id) {
              listener
                .task
                .handle_cmd(TaskCmd::Notify(msg.from, TaskNotify::Rendered));
            }
          }
        }

        KernelCommand::ListenTaskUpdates => {
          self.listeners.insert(msg.from);
        }
        KernelCommand::UnlistenTaskUpdates => {
          self.listeners.remove(&msg.from);
        }
      }
    }
    log::debug!("After kernel loop.");
  }

  fn is_ready_to_quit(&self) -> bool {
    for task in self.tasks.values() {
      match task.status {
        TaskStatus::Running if task.stop_on_quit => return false,
        _ => (),
      }
    }
    true
  }
}
