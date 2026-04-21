use std::{
  collections::{HashMap, HashSet},
  sync::{Arc, atomic::AtomicUsize},
};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{error::ResultLogger, kernel::kernel_message::TaskContext};

use super::{
  kernel_message::{
    KernelCommand, KernelMessage, KernelQuery, KernelQueryResponse, TaskInfo,
  },
  path_trie::PathTrie,
  task::{
    DepInfo, Task, TaskCmd, TaskDef, TaskHandle, TaskId, TaskInit, TaskNotify,
    TaskStatus,
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
  path_trie: PathTrie,
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
      path_trie: PathTrie::new(),
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
    let path = init.path.clone();
    let mut task_handle = TaskHandle {
      task_id,
      task: init.task,

      stop_on_quit: init.stop_on_quit,
      status: init.status,

      deps: HashMap::new(),

      path: init.path,
      vt: None,
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

    if let Some(path) = path {
      if let Err(err) = self.path_trie.insert(&path, task_id) {
        log::error!("Path conflict while spawning task: {}", err);
      }
    }
  }

  pub fn register_task(
    &mut self,
    def: TaskDef,
    factory: impl FnOnce(TaskContext) -> Box<dyn Task> + 'static,
  ) -> TaskId {
    let task_id = TaskId(
      self
        .next_task_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    );
    self.register_task_with_id(task_id, def, Box::new(factory));
    task_id
  }

  fn register_task_with_id(
    &mut self,
    task_id: TaskId,
    def: TaskDef,
    factory: Box<dyn FnOnce(TaskContext) -> Box<dyn Task>>,
  ) {
    let ctx =
      TaskContext::new(self.next_task_id.clone(), task_id, self.sender.clone());
    let task = factory(ctx);
    let path = def.path.clone();
    let mut handle = TaskHandle {
      task_id,
      task,
      stop_on_quit: def.stop_on_quit,
      status: def.status,
      deps: HashMap::new(),
      path: def.path,
      vt: None,
    };

    for dep_id in &def.deps {
      handle.deps.insert(
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

    self.tasks.insert(task_id, handle);

    if let Some(path) = path {
      if let Err(err) = self.path_trie.insert(&path, task_id) {
        log::error!("Path conflict while registering task: {}", err);
      }
    }
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
        KernelCommand::RegisterTask(task_id, def, factory) => {
          self.register_task_with_id(task_id, def, factory);
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

        // TODO: Prevent requeueing.
        KernelCommand::TaskCmdByPath(path, cmd) => {
          if let Some(task_id) = self.path_trie.resolve(&path) {
            self
              .sender
              .send(KernelMessage {
                from: msg.from,
                command: KernelCommand::TaskCmd(task_id, cmd),
              })
              .log_ignore();
          } else {
            log::warn!("No task at path: {}", path);
          }
        }

        KernelCommand::SetTaskPath(task_id, path) => {
          if let Some(task) = self.tasks.get_mut(&task_id) {
            if let Some(old_path) = task.path.take() {
              self.path_trie.remove(&old_path);
            }
            match self.path_trie.insert(&path, task_id) {
              Ok(()) => {
                task.path = Some(path);
              }
              Err(err) => {
                log::error!("Path conflict: {}", err);
              }
            }
          }
        }

        KernelCommand::Query(query, response_tx) => {
          let response = match query {
            KernelQuery::ListTasks(glob) => {
              let entries = match &glob {
                Some(pattern) => self.path_trie.glob(pattern),
                None => self.path_trie.iter(),
              };
              let tasks: Vec<TaskInfo> = entries
                .into_iter()
                .filter_map(|(path, id)| {
                  self.tasks.get(&id).map(|handle| TaskInfo {
                    id,
                    path: Some(path),
                    status: handle.status,
                  })
                })
                .collect();
              KernelQueryResponse::TaskList(tasks)
            }
            KernelQuery::ResolvePath(path) => {
              KernelQueryResponse::ResolvedPath(self.path_trie.resolve(&path))
            }
            KernelQuery::GetScreen(path) => {
              let screen_text =
                self.path_trie.resolve(&path).and_then(|task_id| {
                  self.tasks.get(&task_id).and_then(|handle| {
                    handle.vt.as_ref().and_then(|vt| {
                      vt.read()
                        .ok()
                        .map(|parser| render_screen_ansi(parser.screen()))
                    })
                  })
                });
              KernelQueryResponse::Screen(screen_text)
            }
          };
          let _ = response_tx.send(response);
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
          if let Some(task) = self.tasks.get_mut(&msg.from) {
            task.vt = vt.clone();
          }
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

fn render_screen_ansi(screen: &crate::term::screen::Screen) -> String {
  use std::fmt::Write;

  use crate::term::{attrs::Attrs, color::Color};

  let size = screen.size();
  let mut out = String::new();
  let mut brush = Attrs::default();

  for row in 0..size.height {
    if row > 0 {
      let _ = write!(out, "\r\n");
    }
    let mut line = String::new();
    let mut line_brush = brush;

    for col in 0..size.width {
      let cell = match screen.cell(row, col) {
        Some(c) => c,
        None => continue,
      };
      let attrs = *cell.attrs();

      if line_brush != attrs {
        let _ = write!(line, "\x1b[");
        let mut first = true;
        let mut sep = |w: &mut String| {
          if first {
            first = false;
            Ok(())
          } else {
            write!(w, ";")
          }
        };
        if line_brush.fgcolor != attrs.fgcolor {
          let _ = sep(&mut line);
          match attrs.fgcolor {
            Color::Default => {
              let _ = write!(line, "39");
            }
            Color::Idx(idx) => {
              let _ = write!(line, "38;5;{}", idx);
            }
            Color::Rgb(r, g, b) => {
              let _ = write!(line, "38;2;{r};{g};{b}");
            }
          }
        }
        if line_brush.bgcolor != attrs.bgcolor {
          let _ = sep(&mut line);
          match attrs.bgcolor {
            Color::Default => {
              let _ = write!(line, "49");
            }
            Color::Idx(idx) => {
              let _ = write!(line, "48;5;{}", idx);
            }
            Color::Rgb(r, g, b) => {
              let _ = write!(line, "48;2;{r};{g};{b}");
            }
          }
        }
        if line_brush.bold() != attrs.bold() {
          let _ = sep(&mut line);
          let v = if attrs.bold() { 1 } else { 22 };
          let _ = write!(line, "{v}");
        }
        if line_brush.italic() != attrs.italic() {
          let _ = sep(&mut line);
          let v = if attrs.italic() { 3 } else { 23 };
          let _ = write!(line, "{v}");
        }
        if line_brush.underline() != attrs.underline() {
          let _ = sep(&mut line);
          let v = if attrs.underline() { 4 } else { 24 };
          let _ = write!(line, "{v}");
        }
        if line_brush.inverse() != attrs.inverse() {
          let _ = sep(&mut line);
          let v = if attrs.inverse() { 7 } else { 27 };
          let _ = write!(line, "{v}");
        }
        let _ = write!(line, "m");
        line_brush = attrs;
      }

      let c = if cell.width() > 0 {
        cell.contents()
      } else {
        " "
      };
      line.push_str(c);
    }

    // Trim trailing default-attrs spaces from each line
    out.push_str(line.trim_end());
    brush = line_brush;
  }

  // Reset attributes at the end
  if brush != Attrs::default() {
    let _ = write!(out, "\x1b[0m");
  }

  out
}
