use std::{
  collections::{HashMap, HashSet},
  sync::{Arc, atomic::AtomicUsize},
  time::Instant,
};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{error::ResultLogger, kernel::kernel_message::TaskContext};

use super::{
  kernel_message::{
    KernelCommand, KernelMessage, KernelQuery, KernelQueryResponse, TaskInfo,
  },
  path_trie::PathTrie,
  sub_trie::SubTrie,
  task::{
    DepInfo, Effects, Target, Task, TaskCmd, TaskDef, TaskEffect, TaskHandle,
    TaskId, TaskNotification, TaskNotify, TaskStatus,
  },
  task_path::TaskPath,
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
  sub_trie: SubTrie,
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
      sub_trie: SubTrie::new(),
    }
  }

  pub fn context(&self) -> TaskContext {
    TaskContext::new(self.next_task_id.clone(), TaskId(0), self.sender.clone())
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
    let label = def.label.clone();
    let vt = def.vt.clone();
    let autostart = def.autostart;
    let mut handle = TaskHandle {
      task_id,
      task,
      stop_on_quit: def.stop_on_quit,
      status: def.status,
      pending_start: false,
      autorestart: def.autorestart,
      target: Target::None,
      last_start: None,
      deps: HashMap::new(),
      path: def.path,
      label: def.label,
      vt: def.vt,
    };

    for dep_id in &def.deps {
      handle.deps.insert(
        *dep_id,
        DepInfo {
          status: self
            .tasks
            .get(dep_id)
            .map_or(TaskStatus::NotStarted, |d| d.status),
        },
      );
      self.rev_deps.entry(*dep_id).or_default().insert(task_id);
    }

    let status = handle.status;
    self.tasks.insert(task_id, handle);

    if let Some(ref path) = path {
      if let Err(err) = self.path_trie.insert(path, task_id) {
        log::warn!("Path conflict while registering task: {}", err);
      }
    }

    self.notify_listeners(
      task_id,
      path.clone(),
      TaskNotify::Added {
        path,
        label,
        status,
        vt,
      },
    );

    // Autostart goes through the normal `Start` so dependency gating applies.
    if autostart {
      self
        .sender
        .send(KernelMessage {
          from: TaskId(0),
          command: KernelCommand::TaskCmd(task_id, TaskCmd::Start),
        })
        .log_ignore();
    }
  }

  pub async fn run(mut self) {
    loop {
      let msg = if let Some(msg) = self.receiver.recv().await {
        msg
      } else {
        log::debug!("Kernel receiver returned None.");
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
            let mut fx = Effects::new();
            if let Some(task) = self.tasks.get_mut(&task_id) {
              task.task.handle_cmd(TaskCmd::Stop, &mut fx);
            }
            self.apply_effects(task_id, &mut fx);
          }

          if self.is_ready_to_quit() {
            break;
          }
        }

        KernelCommand::RegisterTask(task_id, def, factory) => {
          self.register_task_with_id(task_id, def, factory);
        }
        KernelCommand::TaskCmd(task_id, cmd) => {
          let mut fx = Effects::new();
          if let Some(task) = self.tasks.get_mut(&task_id) {
            match cmd {
              TaskCmd::Start => {
                task.target = Target::Started;
                if Self::all_deps_ready(task) {
                  task.pending_start = false;
                  task.task.handle_cmd(cmd, &mut fx);
                } else if task.status != TaskStatus::Running {
                  task.pending_start = true;
                }
              }
              TaskCmd::Stop | TaskCmd::Kill => {
                task.target = Target::Stopped;
                task.pending_start = false;
                task.task.handle_cmd(cmd, &mut fx);
              }
              TaskCmd::Msg(_) => {
                task.task.handle_cmd(cmd, &mut fx);
              }
            }
          }
          self.apply_effects(task_id, &mut fx);
          if self.quitting && self.is_ready_to_quit() {
            break;
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

        KernelCommand::RestartTaskByPath(path) => {
          if let Some(task_id) = self.path_trie.resolve(&path) {
            self
              .sender
              .send(KernelMessage {
                from: msg.from,
                command: KernelCommand::RestartTask(task_id),
              })
              .log_ignore();
          } else {
            log::warn!("No task at path: {}", path);
          }
        }

        KernelCommand::RestartTask(task_id) => {
          let mut fx = Effects::new();
          if let Some(task) = self.tasks.get_mut(&task_id) {
            task.target = Target::Started;
            if task.status == TaskStatus::Running {
              task.pending_start = false;
              task.task.handle_cmd(TaskCmd::Stop, &mut fx);
            } else if Self::all_deps_ready(task) {
              task.pending_start = false;
              task.task.handle_cmd(TaskCmd::Start, &mut fx);
            } else {
              task.pending_start = true;
            }
          }
          self.apply_effects(task_id, &mut fx);
        }

        KernelCommand::SetTaskPath(task_id, path) => {
          let taken_by_other = self
            .path_trie
            .resolve(&path)
            .is_some_and(|holder| holder != task_id);
          if !self.tasks.contains_key(&task_id) {
            // Unknown task; nothing to move.
          } else if taken_by_other {
            // Reject up front so the task never loses its current path: only
            // free the old one once the new one is known to be available.
            log::warn!("Path conflict: {} is already taken", path);
          } else {
            let old_path =
              self.tasks.get_mut(&task_id).and_then(|t| t.path.take());
            if let Some(old) = &old_path {
              self.path_trie.remove(old);
            }
            match self.path_trie.insert(&path, task_id) {
              Ok(()) => {
                if let Some(task) = self.tasks.get_mut(&task_id) {
                  task.path = Some(path.clone());
                }
                if old_path.as_ref() != Some(&path) {
                  self.notify_path_changed(task_id, old_path, Some(path));
                }
              }
              Err(err) => {
                log::warn!("Path conflict: {}", err);
              }
            }
          }
        }

        KernelCommand::SetTaskLabel(task_id, label) => {
          let mut changed = false;
          if let Some(task) = self.tasks.get_mut(&task_id) {
            if task.label != label {
              task.label = label.clone();
              changed = true;
            }
          }
          if changed {
            let from_path = self.task_path(task_id);
            self.notify_listeners(
              task_id,
              from_path,
              TaskNotify::LabelChanged(label),
            );
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
                    label: handle.label.clone(),
                    status: handle.status,
                    vt: handle.vt.clone(),
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
          self.apply_effect(msg.from, TaskEffect::Started);
        }
        KernelCommand::TaskStopped(exit_code) => {
          self.apply_effect(msg.from, TaskEffect::Stopped(exit_code));
          if self.quitting && self.is_ready_to_quit() {
            break;
          }
        }
        KernelCommand::RemoveTask(task_id) => {
          self.apply_effect(task_id, TaskEffect::Remove);
          if self.quitting && self.is_ready_to_quit() {
            break;
          }
        }

        KernelCommand::ListenTaskUpdates => {
          self.listeners.insert(msg.from);
        }
        KernelCommand::UnlistenTaskUpdates => {
          self.listeners.remove(&msg.from);
        }

        KernelCommand::SubscribePath(path, mode) => {
          self.sub_trie.subscribe(msg.from, &path, mode);
        }
        KernelCommand::UnsubscribePath(path, mode) => {
          self.sub_trie.unsubscribe(msg.from, &path, mode);
        }
      }
    }
    log::debug!("After kernel loop.");
  }

  fn apply_effects(&mut self, task_id: TaskId, fx: &mut Effects) {
    for effect in fx.drain() {
      self.apply_effect(task_id, effect);
    }
  }

  fn apply_effect(&mut self, task_id: TaskId, effect: TaskEffect) {
    match effect {
      TaskEffect::Started => {
        let mut started = false;
        if let Some(task) = self.tasks.get_mut(&task_id) {
          if task.status != TaskStatus::Running {
            started = true;
          }
          task.status = TaskStatus::Running;
          task.last_start = Some(Instant::now());
          match task.target {
            Target::Started => task.target = Target::None,
            Target::None | Target::Stopped => {}
          }
        }

        if started && let Some(rev_deps) = self.rev_deps.get(&task_id).cloned()
        {
          for rev_dep_id in rev_deps {
            if let Some(rev_dep) = self.tasks.get_mut(&rev_dep_id) {
              if let Some(dep) = rev_dep.deps.get_mut(&task_id) {
                dep.status = TaskStatus::Running;
              }
              if rev_dep.pending_start && Self::all_deps_ready(rev_dep) {
                self
                  .sender
                  .send(KernelMessage {
                    from: TaskId(0),
                    command: KernelCommand::TaskCmd(rev_dep_id, TaskCmd::Start),
                  })
                  .log_ignore();
              }
            }
          }
        }

        let from_path = self.task_path(task_id);
        self.notify_listeners(task_id, from_path, TaskNotify::Started);
      }

      TaskEffect::Stopped(exit_code) => {
        let mut restart = false;
        if let Some(task) = self.tasks.get_mut(&task_id) {
          task.status = TaskStatus::Exited(exit_code);
          task.pending_start = false;
          restart = decide_restart(
            task.target,
            task.autorestart,
            exit_code,
            task.last_start,
          );
          match task.target {
            Target::Stopped => task.target = Target::None,
            Target::None | Target::Started => {}
          }
        }

        if let Some(rev_deps) = self.rev_deps.get(&task_id).cloned() {
          for rev_dep_id in rev_deps {
            if let Some(rev_dep) = self.tasks.get_mut(&rev_dep_id)
              && let Some(dep) = rev_dep.deps.get_mut(&task_id)
            {
              dep.status = TaskStatus::Exited(exit_code);
            }
          }
        }
        let from_path = self.task_path(task_id);
        self.notify_listeners(
          task_id,
          from_path,
          TaskNotify::Stopped(exit_code),
        );

        if restart {
          self
            .sender
            .send(KernelMessage {
              from: TaskId(0),
              command: KernelCommand::TaskCmd(task_id, TaskCmd::Start),
            })
            .log_ignore();
        }
      }

      TaskEffect::Remove => {
        let from_path = self.task_path(task_id);
        if let Some(handle) = self.tasks.remove(&task_id) {
          if let Some(path) = &handle.path {
            self.path_trie.remove(path);
          }

          for dep_id in handle.deps.keys() {
            if let Some(set) = self.rev_deps.get_mut(dep_id) {
              set.remove(&task_id);
            }
          }

          if let Some(dependents) = self.rev_deps.remove(&task_id) {
            for dep_id in &dependents {
              if let Some(other) = self.tasks.get_mut(dep_id) {
                other.deps.remove(&task_id);
              }
            }
          }

          self.listeners.remove(&task_id);
          self.sub_trie.remove_subscriber(task_id);
        }

        self.notify_listeners(task_id, from_path, TaskNotify::Removed);
      }
    }
  }

  fn task_path(&self, task_id: TaskId) -> Option<TaskPath> {
    self.tasks.get(&task_id).and_then(|t| t.path.clone())
  }

  fn notify_listeners(
    &mut self,
    from: TaskId,
    from_path: Option<TaskPath>,
    notify: TaskNotify,
  ) {
    let mut targets = self.listeners.clone();
    if let Some(path) = &from_path {
      self.sub_trie.collect(path, &mut targets);
    }
    self.deliver(from, from_path, notify, targets);
  }

  /// Translate a path change into each subscriber's own vocabulary: a task
  /// moving into scope looks like `Added` (with full status/vt), out of scope
  /// like `Removed`, and a move within scope like a rename.
  fn notify_path_changed(
    &mut self,
    from: TaskId,
    old: Option<TaskPath>,
    new: Option<TaskPath>,
  ) {
    let (status, label, vt) = match self.tasks.get(&from) {
      Some(t) => (t.status, t.label.clone(), t.vt.clone()),
      None => return,
    };

    let mut old_targets = self.listeners.clone();
    if let Some(old) = &old {
      self.sub_trie.collect(old, &mut old_targets);
    }
    let mut new_targets = self.listeners.clone();
    if let Some(new) = &new {
      self.sub_trie.collect(new, &mut new_targets);
    }

    let entering: HashSet<TaskId> =
      new_targets.difference(&old_targets).copied().collect();
    let leaving: HashSet<TaskId> =
      old_targets.difference(&new_targets).copied().collect();
    let staying: HashSet<TaskId> =
      old_targets.intersection(&new_targets).copied().collect();

    if let Some(new) = &new {
      self.deliver(
        from,
        Some(new.clone()),
        TaskNotify::Added {
          path: Some(new.clone()),
          label,
          status,
          vt,
        },
        entering,
      );
    }
    self.deliver(from, old.clone(), TaskNotify::Removed, leaving);
    self.deliver(
      from,
      new.clone().or_else(|| old.clone()),
      TaskNotify::PathChanged(old, new),
      staying,
    );
  }

  fn deliver(
    &mut self,
    from: TaskId,
    from_path: Option<TaskPath>,
    notify: TaskNotify,
    targets: HashSet<TaskId>,
  ) {
    let mut all_fx: Vec<(TaskId, Effects)> = Vec::new();
    for listener_id in targets {
      if let Some(listener) = self.tasks.get_mut(&listener_id) {
        let mut fx = Effects::new();
        listener.task.handle_cmd(
          TaskCmd::msg(TaskNotification {
            from,
            from_path: from_path.clone(),
            notify: notify.clone(),
          }),
          &mut fx,
        );
        all_fx.push((listener_id, fx));
      }
    }
    for (task_id, mut fx) in all_fx {
      self.apply_effects(task_id, &mut fx);
    }
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

  fn all_deps_ready(task: &TaskHandle) -> bool {
    task
      .deps
      .values()
      .all(|dep| dep.status == TaskStatus::Running)
  }
}

/// Minimum uptime before an unexpected exit qualifies for autorestart, so a
/// process that fails immediately isn't restarted in a tight loop.
const RESTART_THRESHOLD_SECONDS: f64 = 1.0;

fn decide_restart(
  target: Target,
  autorestart: bool,
  code: u32,
  last_start: Option<Instant>,
) -> bool {
  match target {
    Target::Started => true,
    Target::Stopped => false,
    Target::None => {
      autorestart
        && code != 0
        && last_start.map_or(true, |t| {
          Instant::now().duration_since(t).as_secs_f64()
            > RESTART_THRESHOLD_SECONDS
        })
    }
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

#[cfg(test)]
mod tests {
  use std::time::{Duration, Instant};

  use tokio::sync::mpsc::{
    UnboundedReceiver, error::TryRecvError, unbounded_channel,
  };

  use super::*;

  #[test]
  fn autorestart_policy() {
    let recent = Instant::now();
    let old = Instant::now() - Duration::from_secs(2);

    // Explicit intent overrides the autorestart policy.
    assert!(decide_restart(Target::Started, false, 0, Some(recent)));
    assert!(!decide_restart(Target::Stopped, true, 1, Some(old)));

    // No intent: restart only on a nonzero exit, with autorestart on, after
    // the task has stayed up past the threshold.
    assert!(decide_restart(Target::None, true, 1, Some(old)));
    assert!(decide_restart(Target::None, true, 1, None));
    assert!(!decide_restart(Target::None, true, 1, Some(recent))); // too brief
    assert!(!decide_restart(Target::None, true, 0, Some(old))); // clean exit
    assert!(!decide_restart(Target::None, false, 1, Some(old))); // disabled
  }

  #[derive(Debug, PartialEq)]
  enum RecordedCmd {
    Start,
    Stop,
    Kill,
  }

  struct RecordingTask {
    tx: UnboundedSender<RecordedCmd>,
  }

  impl Task for RecordingTask {
    fn handle_cmd(&mut self, cmd: TaskCmd, fx: &mut Effects) {
      match cmd {
        TaskCmd::Start => {
          self.tx.send(RecordedCmd::Start).unwrap();
          fx.started();
        }
        TaskCmd::Stop => {
          self.tx.send(RecordedCmd::Stop).unwrap();
          fx.stopped(0);
        }
        TaskCmd::Kill => {
          self.tx.send(RecordedCmd::Kill).unwrap();
          fx.stopped(137);
        }
        TaskCmd::Msg(_) => {}
      }
    }
  }

  fn recording_task() -> (
    UnboundedReceiver<RecordedCmd>,
    impl FnOnce(TaskContext) -> Box<dyn Task> + 'static,
  ) {
    let (tx, rx) = unbounded_channel();
    (rx, move |_| Box::new(RecordingTask { tx }))
  }

  async fn recv_cmd(rx: &mut UnboundedReceiver<RecordedCmd>) -> RecordedCmd {
    tokio::time::timeout(Duration::from_secs(1), rx.recv())
      .await
      .expect("timed out waiting for task command")
      .expect("task command channel closed")
  }

  async fn flush_kernel(pc: &TaskContext) {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    pc.send(KernelCommand::Query(
      KernelQuery::ListTasks(None),
      response_tx,
    ));
    tokio::time::timeout(Duration::from_secs(1), response_rx)
      .await
      .expect("timed out waiting for kernel query response")
      .expect("kernel query response channel closed");
  }

  fn assert_no_cmd(rx: &mut UnboundedReceiver<RecordedCmd>) {
    match rx.try_recv() {
      Ok(cmd) => panic!("unexpected task command: {cmd:?}"),
      Err(TryRecvError::Disconnected) => panic!("task command channel closed"),
      Err(TryRecvError::Empty) => {}
    }
  }

  async fn label_of(pc: &TaskContext, id: TaskId) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    pc.send(KernelCommand::Query(KernelQuery::ListTasks(None), tx));
    let resp = tokio::time::timeout(Duration::from_secs(1), rx)
      .await
      .expect("timed out listing tasks")
      .expect("kernel query channel closed");
    match resp {
      KernelQueryResponse::TaskList(list) => {
        list.into_iter().find(|t| t.id == id).and_then(|t| t.label)
      }
      _ => panic!("unexpected query response"),
    }
  }

  #[tokio::test]
  async fn task_label_is_stored_and_updatable() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (_rx, factory) = recording_task();
    // The label may hold characters that aren't valid in a path (spaces).
    let id = kernel.register_task(
      TaskDef {
        path: Some(TaskPath::new("/1").unwrap()),
        label: Some("web server".to_string()),
        ..Default::default()
      },
      factory,
    );

    let kernel_task = tokio::spawn(kernel.run());

    assert_eq!(label_of(&pc, id).await.as_deref(), Some("web server"));

    pc.send(KernelCommand::SetTaskLabel(id, Some("renamed".to_string())));
    assert_eq!(label_of(&pc, id).await.as_deref(), Some("renamed"));

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  async fn resolve(pc: &TaskContext, path: &str) -> Option<TaskId> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    pc.send(KernelCommand::Query(
      KernelQuery::ResolvePath(TaskPath::new(path).unwrap()),
      tx,
    ));
    let resp = tokio::time::timeout(Duration::from_secs(1), rx)
      .await
      .expect("timed out resolving path")
      .expect("kernel query channel closed");
    match resp {
      KernelQueryResponse::ResolvedPath(id) => id,
      _ => panic!("unexpected query response"),
    }
  }

  fn path_task(
    path: &str,
  ) -> (
    UnboundedReceiver<RecordedCmd>,
    TaskDef,
    impl FnOnce(TaskContext) -> Box<dyn Task> + 'static,
  ) {
    let (rx, factory) = recording_task();
    let def = TaskDef {
      path: Some(TaskPath::new(path).unwrap()),
      ..Default::default()
    };
    (rx, def, factory)
  }

  #[tokio::test]
  async fn set_task_path_rejects_conflict_and_keeps_old_path() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (_a_rx, a_def, a_task) = path_task("/a");
    let a = kernel.register_task(a_def, a_task);
    let (_b_rx, b_def, b_task) = path_task("/b");
    let b = kernel.register_task(b_def, b_task);

    let kernel_task = tokio::spawn(kernel.run());

    // Target taken by another task: rejected, both paths intact.
    pc.send(KernelCommand::SetTaskPath(a, TaskPath::new("/b").unwrap()));
    assert_eq!(resolve(&pc, "/a").await, Some(a));
    assert_eq!(resolve(&pc, "/b").await, Some(b));

    // Free target: moves cleanly, old path released.
    pc.send(KernelCommand::SetTaskPath(a, TaskPath::new("/c").unwrap()));
    assert_eq!(resolve(&pc, "/c").await, Some(a));
    assert_eq!(resolve(&pc, "/a").await, None);

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  #[tokio::test]
  async fn repro_dependency_start_does_not_start_unrequested_dependent() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (mut provider_rx, provider_task) = recording_task();
    let provider_id = kernel.register_task(TaskDef::default(), provider_task);

    let (mut dependent_rx, dependent_task) = recording_task();
    kernel.register_task(
      TaskDef {
        deps: vec![provider_id],
        ..Default::default()
      },
      dependent_task,
    );

    let kernel_task = tokio::spawn(kernel.run());

    pc.send(KernelCommand::TaskCmd(provider_id, TaskCmd::Start));

    assert_eq!(recv_cmd(&mut provider_rx).await, RecordedCmd::Start);
    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  #[tokio::test]
  async fn repro_stopped_dependency_blocks_later_dependent_start() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (mut provider_rx, provider_task) = recording_task();
    let provider_id = kernel.register_task(
      TaskDef {
        status: TaskStatus::Running,
        ..Default::default()
      },
      provider_task,
    );

    let (mut dependent_rx, dependent_task) = recording_task();
    let dependent_id = kernel.register_task(
      TaskDef {
        deps: vec![provider_id],
        ..Default::default()
      },
      dependent_task,
    );

    let kernel_task = tokio::spawn(kernel.run());

    pc.send(KernelCommand::TaskCmd(provider_id, TaskCmd::Stop));

    assert_eq!(recv_cmd(&mut provider_rx).await, RecordedCmd::Stop);

    pc.send(KernelCommand::TaskCmd(dependent_id, TaskCmd::Start));

    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  #[tokio::test]
  async fn requested_dependent_starts_when_dependency_becomes_running() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (mut provider_rx, provider_task) = recording_task();
    let provider_id = kernel.register_task(TaskDef::default(), provider_task);

    let (mut dependent_rx, dependent_task) = recording_task();
    let dependent_id = kernel.register_task(
      TaskDef {
        deps: vec![provider_id],
        ..Default::default()
      },
      dependent_task,
    );

    let kernel_task = tokio::spawn(kernel.run());

    pc.send(KernelCommand::TaskCmd(dependent_id, TaskCmd::Start));
    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::TaskCmd(provider_id, TaskCmd::Start));

    assert_eq!(recv_cmd(&mut provider_rx).await, RecordedCmd::Start);
    assert_eq!(recv_cmd(&mut dependent_rx).await, RecordedCmd::Start);

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  #[tokio::test]
  async fn pending_dependent_start_survives_task_msg() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (mut provider_rx, provider_task) = recording_task();
    let provider_id = kernel.register_task(TaskDef::default(), provider_task);

    let (mut dependent_rx, dependent_task) = recording_task();
    let dependent_id = kernel.register_task(
      TaskDef {
        deps: vec![provider_id],
        ..Default::default()
      },
      dependent_task,
    );

    let kernel_task = tokio::spawn(kernel.run());

    pc.send(KernelCommand::TaskCmd(dependent_id, TaskCmd::Start));
    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::TaskCmd(dependent_id, TaskCmd::msg(())));
    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::TaskCmd(provider_id, TaskCmd::Start));

    assert_eq!(recv_cmd(&mut provider_rx).await, RecordedCmd::Start);
    assert_eq!(recv_cmd(&mut dependent_rx).await, RecordedCmd::Start);

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  #[tokio::test]
  async fn stop_cancels_pending_dependent_start() {
    let mut kernel = Kernel::new();
    let pc = kernel.context();

    let (mut provider_rx, provider_task) = recording_task();
    let provider_id = kernel.register_task(TaskDef::default(), provider_task);

    let (mut dependent_rx, dependent_task) = recording_task();
    let dependent_id = kernel.register_task(
      TaskDef {
        deps: vec![provider_id],
        ..Default::default()
      },
      dependent_task,
    );

    let kernel_task = tokio::spawn(kernel.run());

    pc.send(KernelCommand::TaskCmd(dependent_id, TaskCmd::Start));
    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::TaskCmd(dependent_id, TaskCmd::Stop));
    assert_eq!(recv_cmd(&mut dependent_rx).await, RecordedCmd::Stop);

    pc.send(KernelCommand::TaskCmd(provider_id, TaskCmd::Start));

    assert_eq!(recv_cmd(&mut provider_rx).await, RecordedCmd::Start);
    flush_kernel(&pc).await;
    assert_no_cmd(&mut dependent_rx);

    pc.send(KernelCommand::Quit);
    tokio::time::timeout(Duration::from_secs(1), kernel_task)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }
}
