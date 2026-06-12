use std::{
  collections::{HashMap, HashSet},
  sync::{Arc, atomic::AtomicUsize},
  time::{Duration, Instant},
};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::kernel::kernel_message::TaskContext;

use super::{
  kernel_message::{
    DepExplain, KernelCommand, KernelMessage, KernelQuery, KernelQueryResponse,
    TaskExplain, TaskInfo,
  },
  path_trie::PathTrie,
  sub_trie::SubTrie,
  task::{
    Effects, ExitInfo, INIT_TASK_ID, ReadyMode, RestartMode, Task, TaskCmd,
    TaskDef, TaskEffect, TaskHandle, TaskId, TaskKind, TaskNotification,
    TaskNotify, TaskState,
  },
  task_path::TaskPath,
};

/// How long a stopping task may take before it is hard-killed.
const STOP_GRACE: Duration = Duration::from_secs(10);

const BACKOFF_MIN: Duration = Duration::from_millis(100);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// Uptime after which the restart attempt counter resets.
const BACKOFF_RESET: Duration = Duration::from_secs(10);

fn backoff_delay(attempts: u32) -> Duration {
  let exp = attempts.saturating_sub(1).min(16);
  BACKOFF_MIN.saturating_mul(1 << exp).min(BACKOFF_MAX)
}

pub struct Kernel {
  sender: UnboundedSender<KernelMessage>,
  receiver: UnboundedReceiver<KernelMessage>,

  quitting: bool,
  next_task_id: Arc<AtomicUsize>,
  tasks: HashMap<TaskId, TaskHandle>,
  /// `edges[a]` contains `b` when `a` requires `b`. Edges from
  /// `INIT_TASK_ID` are pins.
  edges: HashMap<TaskId, HashSet<TaskId>>,
  /// Reverse of `edges`: `redges[b]` contains `a` when `a` requires `b`.
  redges: HashMap<TaskId, HashSet<TaskId>>,
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
      edges: HashMap::new(),
      redges: HashMap::new(),
      path_trie: PathTrie::new(),
      sub_trie: SubTrie::new(),
    }
  }

  pub fn context(&self) -> TaskContext {
    TaskContext::new(
      self.next_task_id.clone(),
      INIT_TASK_ID,
      self.sender.clone(),
    )
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
    if self.tasks.contains_key(&task_id) {
      log::warn!("Duplicate task id {:?}; registration ignored", task_id);
      return;
    }
    let ctx =
      TaskContext::new(self.next_task_id.clone(), task_id, self.sender.clone());
    let task = factory(ctx);
    // The handle only keeps the path if the trie insert succeeds, so the
    // handle and the trie owner never disagree.
    // TODO: Probaly should fail if path already exists.
    let path = match def.path {
      Some(p) => match self.path_trie.insert(&p, task_id) {
        Ok(()) => Some(p),
        Err(err) => {
          log::warn!("Path conflict while registering task: {}", err);
          None
        }
      },
      None => None,
    };
    let label = def.label.clone();
    let vt = def.vt.clone();
    let handle = TaskHandle {
      task,
      state: TaskState::Idle,
      epoch: 0,
      kept_down: false,
      killed: false,
      attempts: 0,
      last_start: None,
      kind: def.kind,
      ready: def.ready,
      restart: def.restart,
      path: path.clone(),
      label: def.label,
      vt: def.vt,
    };
    self.tasks.insert(task_id, handle);

    for dep_id in def.deps {
      self.add_edge(task_id, dep_id);
    }
    if def.pinned {
      self.add_edge(INIT_TASK_ID, task_id);
    }

    self.notify_subscribers(
      task_id,
      path.clone(),
      TaskNotify::Added {
        path,
        label,
        state: TaskState::Idle,
        vt,
      },
    );
  }

  pub async fn run(mut self) {
    loop {
      let msg = if let Some(msg) = self.receiver.recv().await {
        msg
      } else {
        log::debug!("Kernel receiver returned None.");
        break;
      };

      if self.handle_message(msg) {
        break;
      }
      self.reconcile();
      if self.quitting && self.no_active_tasks() {
        break;
      }
    }
    log::debug!("After kernel loop.");
  }

  /// Returns true when the kernel loop should exit immediately.
  fn handle_message(&mut self, msg: KernelMessage) -> bool {
    match msg.command {
      KernelCommand::Quit => {
        if self.quitting {
          return true;
        }
        self.quitting = true;
      }

      KernelCommand::RegisterTask(task_id, def, factory) => {
        self.register_task_with_id(task_id, def, factory);
      }
      KernelCommand::RemoveTask(task_id) => {
        self.remove_task(task_id);
      }

      KernelCommand::Start(task_id) => self.cmd_start(task_id),
      KernelCommand::Stop(task_id) => self.cmd_stop(task_id),
      KernelCommand::Kill(task_id) => self.cmd_kill(task_id),
      KernelCommand::KeepDown(task_id) => self.cmd_keep_down(task_id),
      KernelCommand::Restart(task_id) => self.cmd_restart(task_id),
      KernelCommand::Down(task_id) => {
        self.remove_edge(INIT_TASK_ID, task_id);
      }
      KernelCommand::AddEdge { from, to } => self.add_edge(from, to),
      KernelCommand::RemoveEdge { from, to } => self.remove_edge(from, to),

      KernelCommand::TaskMsg(task_id, m) => {
        self.send_cmd(task_id, TaskCmd::Msg(m));
      }

      KernelCommand::SetTaskPath(task_id, path) => {
        self.set_task_path(task_id, path);
      }

      KernelCommand::SetTaskLabel(task_id, label) => {
        self.set_task_label(task_id, label);
      }

      KernelCommand::Query(query, response_tx) => {
        let _ = response_tx.send(self.handle_query(query));
      }

      KernelCommand::TaskStarted => self.on_task_started(msg.from),
      KernelCommand::TaskReady => self.on_task_ready(msg.from),
      KernelCommand::TaskStopped(info) => self.on_task_stopped(msg.from, info),

      KernelCommand::StateTimeout(task_id, epoch) => {
        self.on_state_timeout(task_id, epoch)
      }

      KernelCommand::SubscribePath(path, mode) => {
        self.sub_trie.subscribe(msg.from, &path, mode);
      }
      KernelCommand::UnsubscribePath(path, mode) => {
        self.sub_trie.unsubscribe(msg.from, &path, mode);
      }
    }
    false
  }

  // ---- Intent ----

  fn cmd_start(&mut self, task_id: TaskId) {
    if !self.tasks.contains_key(&task_id) {
      log::warn!("Start: unknown task {:?}", task_id);
      return;
    }
    self.add_edge(INIT_TASK_ID, task_id);
    self.demand(task_id);
  }

  /// An explicit start demands the whole requirement closure: kept-down
  /// tasks are released and dead deps revived, so the pull is never blocked
  /// by an earlier stop or crash. Done jobs stay done unless directly
  /// targeted.
  fn demand(&mut self, task_id: TaskId) {
    let mut closure = vec![task_id];
    let mut seen: HashSet<TaskId> = HashSet::from([task_id]);
    let mut i = 0;
    while i < closure.len() {
      if let Some(deps) = self.edges.get(&closure[i]) {
        for dep in deps {
          if seen.insert(*dep) {
            closure.push(*dep);
          }
        }
      }
      i += 1;
    }
    for id in closure {
      let Some(task) = self.tasks.get_mut(&id) else {
        continue;
      };
      task.kept_down = false;
      let revive = match task.state {
        TaskState::Backoff | TaskState::Exited(_) => true,
        TaskState::Done(_) => id == task_id,
        TaskState::Idle
        | TaskState::Starting
        | TaskState::Running
        | TaskState::Ready
        | TaskState::Stopping => false,
      };
      if revive {
        self.set_state(id, TaskState::Idle);
      }
    }
  }

  fn cmd_stop(&mut self, task_id: TaskId) {
    self.remove_edge(INIT_TASK_ID, task_id);
    self.stop_if_active(task_id);
  }

  fn cmd_kill(&mut self, task_id: TaskId) {
    self.remove_edge(INIT_TASK_ID, task_id);
    let Some(task) = self.tasks.get(&task_id) else {
      return;
    };
    match task.state {
      TaskState::Starting | TaskState::Running | TaskState::Ready => {
        self.set_state(task_id, TaskState::Stopping);
        self.hard_kill(task_id);
      }
      TaskState::Stopping => {
        // Already stopping gracefully: kill now.
        self.hard_kill(task_id);
      }
      TaskState::Idle
      | TaskState::Backoff
      | TaskState::Done(_)
      | TaskState::Exited(_) => (),
    }
  }

  fn cmd_keep_down(&mut self, task_id: TaskId) {
    self.remove_edge(INIT_TASK_ID, task_id);
    if let Some(task) = self.tasks.get_mut(&task_id) {
      task.kept_down = true;
    }
  }

  fn cmd_restart(&mut self, task_id: TaskId) {
    self.cmd_start(task_id);
    self.stop_if_active(task_id);
  }

  fn stop_if_active(&mut self, task_id: TaskId) {
    let Some(task) = self.tasks.get(&task_id) else {
      return;
    };
    match task.state {
      TaskState::Starting | TaskState::Running | TaskState::Ready => {
        self.stop_task(task_id);
      }
      TaskState::Idle
      | TaskState::Stopping
      | TaskState::Backoff
      | TaskState::Done(_)
      | TaskState::Exited(_) => (),
    }
  }

  // ---- Edges ----

  fn add_edge(&mut self, from: TaskId, to: TaskId) {
    if from == to || to == INIT_TASK_ID {
      log::warn!("Invalid edge: {:?} -> {:?}", from, to);
      return;
    }
    if !self.tasks.contains_key(&to)
      || (from != INIT_TASK_ID && !self.tasks.contains_key(&from))
    {
      log::warn!("Edge references unknown task: {:?} -> {:?}", from, to);
      return;
    }
    if self.reaches(to, from) {
      log::warn!("Edge would create a cycle: {:?} -> {:?}", from, to);
      return;
    }
    self.edges.entry(from).or_default().insert(to);
    self.redges.entry(to).or_default().insert(from);
  }

  fn remove_edge(&mut self, from: TaskId, to: TaskId) {
    if let Some(set) = self.edges.get_mut(&from) {
      set.remove(&to);
      if set.is_empty() {
        self.edges.remove(&from);
      }
    }
    if let Some(set) = self.redges.get_mut(&to) {
      set.remove(&from);
      if set.is_empty() {
        self.redges.remove(&to);
      }
    }
  }

  /// Whether `to` is reachable from `from` following edges.
  fn reaches(&self, from: TaskId, to: TaskId) -> bool {
    let mut seen: HashSet<TaskId> = HashSet::from([from]);
    let mut stack = vec![from];
    while let Some(id) = stack.pop() {
      if id == to {
        return true;
      }
      if let Some(nexts) = self.edges.get(&id) {
        for next in nexts {
          if seen.insert(*next) {
            stack.push(*next);
          }
        }
      }
    }
    false
  }

  /// Tasks that should be up: reachable from init.
  fn wanted_set(&self) -> HashSet<TaskId> {
    let mut wanted = HashSet::new();
    if self.quitting {
      return wanted;
    }
    let mut seen: HashSet<TaskId> = HashSet::from([INIT_TASK_ID]);
    let mut stack = vec![INIT_TASK_ID];
    while let Some(from) = stack.pop() {
      let Some(tos) = self.edges.get(&from) else {
        continue;
      };
      for to in tos {
        if !seen.insert(*to) {
          continue;
        }
        let Some(task) = self.tasks.get(to) else {
          continue;
        };
        if task.kept_down {
          continue;
        }
        wanted.insert(*to);
        stack.push(*to);
      }
    }
    wanted
  }

  // ---- Reconciliation ----

  fn reconcile(&mut self) {
    // Each pass can unlock more work (a task stopping synchronously lets
    // its deps stop); repeat until a pass changes nothing.
    let limit = self.tasks.len() * 2 + 8;
    for _ in 0..limit {
      if !self.reconcile_pass() {
        return;
      }
    }
    log::warn!("Reconcile did not settle after {} passes", limit);
  }

  fn reconcile_pass(&mut self) -> bool {
    let wanted = self.wanted_set();
    let supported = self.supported_set(&wanted);
    let ids: Vec<TaskId> = self.tasks.keys().copied().collect();
    let mut acted = false;
    for id in ids {
      let Some(task) = self.tasks.get(&id) else {
        continue;
      };
      let state = task.state;
      if supported.contains(&id) {
        match state {
          TaskState::Idle => {
            self.start_task(id);
            acted = true;
          }
          TaskState::Starting
          | TaskState::Running
          | TaskState::Ready
          | TaskState::Stopping
          | TaskState::Backoff
          | TaskState::Done(_)
          | TaskState::Exited(_) => (),
        }
      } else {
        match state {
          TaskState::Starting | TaskState::Running | TaskState::Ready => {
            // Ordered shutdown: dependents go down first.
            if !self.has_active_dependent(id) {
              self.stop_task(id);
              acted = true;
            }
          }
          TaskState::Backoff => {
            // Cancel a pending retry.
            self.set_state(id, TaskState::Idle);
          }
          TaskState::Idle
          | TaskState::Stopping
          | TaskState::Done(_)
          | TaskState::Exited(_) => (),
        }
      }
    }
    acted
  }

  /// Tasks that may run right now: wanted, with every dependency
  /// transitively supported and currently satisfied.
  fn supported_set(&self, wanted: &HashSet<TaskId>) -> HashSet<TaskId> {
    let mut memo: HashMap<TaskId, bool> = HashMap::new();
    for id in self.tasks.keys() {
      self.supported(*id, wanted, &mut memo);
    }
    memo
      .into_iter()
      .filter_map(|(id, ok)| if ok { Some(id) } else { None })
      .collect()
  }

  fn supported(
    &self,
    id: TaskId,
    wanted: &HashSet<TaskId>,
    memo: &mut HashMap<TaskId, bool>,
  ) -> bool {
    if let Some(ok) = memo.get(&id) {
      return *ok;
    }
    let mut ok = wanted.contains(&id);
    if ok && let Some(deps) = self.edges.get(&id) {
      for dep in deps {
        if !self.supported(*dep, wanted, memo)
          || !self.tasks.get(dep).is_some_and(|t| t.is_satisfied())
        {
          ok = false;
          break;
        }
      }
    }
    memo.insert(id, ok);
    ok
  }

  fn has_active_dependent(&self, task_id: TaskId) -> bool {
    let Some(dependents) = self.redges.get(&task_id) else {
      return false;
    };
    dependents.iter().any(|from| {
      *from != INIT_TASK_ID
        && self.tasks.get(from).is_some_and(|t| t.state.is_active())
    })
  }

  fn no_active_tasks(&self) -> bool {
    self.tasks.values().all(|task| !task.state.is_active())
  }

  // ---- Driving tasks ----

  fn start_task(&mut self, task_id: TaskId) {
    if let Some(task) = self.tasks.get_mut(&task_id) {
      task.last_start = Some(Instant::now());
    }
    self.set_state(task_id, TaskState::Starting);
    self.send_cmd(task_id, TaskCmd::Start);
  }

  fn stop_task(&mut self, task_id: TaskId) {
    self.set_state(task_id, TaskState::Stopping);
    let epoch = match self.tasks.get(&task_id) {
      Some(task) => task.epoch,
      None => return,
    };
    self.schedule_state_timeout(task_id, epoch, STOP_GRACE);
    self.send_cmd(task_id, TaskCmd::Stop);
  }

  fn hard_kill(&mut self, task_id: TaskId) {
    let epoch = {
      let Some(task) = self.tasks.get_mut(&task_id) else {
        return;
      };
      task.killed = true;
      // Manual bump: invalidates the pending stop-grace timeout.
      task.epoch += 1;
      task.epoch
    };
    self.send_cmd(task_id, TaskCmd::Kill);
    self.schedule_state_timeout(task_id, epoch, STOP_GRACE);
  }

  fn send_cmd(&mut self, task_id: TaskId, cmd: TaskCmd) {
    let mut fx = Effects::new();
    if let Some(task) = self.tasks.get_mut(&task_id) {
      task.task.handle_cmd(cmd, &mut fx);
    }
    self.apply_effects(task_id, &mut fx);
  }

  fn schedule_state_timeout(
    &self,
    task_id: TaskId,
    epoch: u64,
    delay: Duration,
  ) {
    let sender = self.sender.clone();
    tokio::spawn(async move {
      tokio::time::sleep(delay).await;
      let _ = sender.send(KernelMessage {
        from: INIT_TASK_ID,
        command: KernelCommand::StateTimeout(task_id, epoch),
      });
    });
  }

  fn on_state_timeout(&mut self, task_id: TaskId, epoch: u64) {
    let Some(task) = self.tasks.get(&task_id) else {
      return;
    };
    if task.epoch != epoch {
      return;
    }
    match task.state {
      TaskState::Backoff => {
        // Retry delay is over; the reconciler restarts it if still wanted.
        self.set_state(task_id, TaskState::Idle);
      }
      TaskState::Stopping => {
        if task.killed {
          // The task ignored a hard kill for a full grace period; stop
          // waiting so the graph (and quit) can make progress.
          log::warn!("Task {:?} did not stop after kill; giving up", task_id);
          self.set_state(task_id, TaskState::Exited(ExitInfo::error()));
        } else {
          // The stop was ignored for the whole grace period: hard-kill.
          self.hard_kill(task_id);
        }
      }
      TaskState::Idle
      | TaskState::Starting
      | TaskState::Running
      | TaskState::Ready
      | TaskState::Done(_)
      | TaskState::Exited(_) => (),
    }
  }

  // ---- Task reports ----

  fn on_task_started(&mut self, task_id: TaskId) {
    let Some(task) = self.tasks.get(&task_id) else {
      return;
    };
    match task.state {
      TaskState::Starting => {
        let state = match task.ready {
          ReadyMode::Immediate => TaskState::Ready,
          ReadyMode::Reported => TaskState::Running,
        };
        self.set_state(task_id, state);
      }
      TaskState::Idle
      | TaskState::Running
      | TaskState::Ready
      | TaskState::Stopping
      | TaskState::Backoff
      | TaskState::Done(_)
      | TaskState::Exited(_) => {
        log::debug!("Ignoring started report in {:?}", task.state);
      }
    }
  }

  fn on_task_ready(&mut self, task_id: TaskId) {
    let Some(task) = self.tasks.get(&task_id) else {
      return;
    };
    match task.state {
      TaskState::Running => self.set_state(task_id, TaskState::Ready),
      TaskState::Idle
      | TaskState::Starting
      | TaskState::Ready
      | TaskState::Stopping
      | TaskState::Backoff
      | TaskState::Done(_)
      | TaskState::Exited(_) => {
        log::debug!("Ignoring ready report in {:?}", task.state);
      }
    }
  }

  fn on_task_stopped(&mut self, task_id: TaskId, info: ExitInfo) {
    let Some(task) = self.tasks.get_mut(&task_id) else {
      return;
    };
    match task.state {
      TaskState::Stopping => {
        // A commanded stop always lands in Idle; the reconciler decides what
        // happens next from intent.
        self.set_state(task_id, TaskState::Idle);
      }
      TaskState::Starting | TaskState::Running | TaskState::Ready => {
        let uptime = task.last_start.map(|t| t.elapsed());
        if uptime.is_some_and(|t| t > BACKOFF_RESET) {
          task.attempts = 0;
        }
        if task.kind == TaskKind::Job && info.success() {
          self.set_state(task_id, TaskState::Done(info));
          return;
        }
        let restart = match task.restart {
          RestartMode::Never => false,
          RestartMode::OnFailure => !info.success(),
          RestartMode::Always => true,
        };
        if restart {
          task.attempts += 1;
          let delay = backoff_delay(task.attempts);
          self.set_state(task_id, TaskState::Backoff);
          if let Some(task) = self.tasks.get(&task_id) {
            self.schedule_state_timeout(task_id, task.epoch, delay);
          }
        } else {
          self.set_state(task_id, TaskState::Exited(info));
        }
      }
      TaskState::Idle
      | TaskState::Backoff
      | TaskState::Done(_)
      | TaskState::Exited(_) => {
        log::debug!("Ignoring stop report in {:?}", task.state);
      }
    }
  }

  // ---- State / bookkeeping ----

  fn set_state(&mut self, task_id: TaskId, state: TaskState) {
    let Some(task) = self.tasks.get_mut(&task_id) else {
      return;
    };
    if task.state == state {
      return;
    }
    task.state = state;
    task.epoch += 1;
    task.killed = false;
    let path = task.path.clone();
    self.notify_subscribers(task_id, path, TaskNotify::StateChanged(state));
  }

  fn remove_task(&mut self, task_id: TaskId) {
    let Some(mut handle) = self.tasks.remove(&task_id) else {
      return;
    };
    if handle.state.is_active() {
      let mut fx = Effects::new();
      handle.task.handle_cmd(TaskCmd::Kill, &mut fx);
    }
    if let Some(path) = &handle.path {
      self.path_trie.remove(path);
    }

    if let Some(deps) = self.edges.remove(&task_id) {
      for dep in deps {
        if let Some(set) = self.redges.get_mut(&dep) {
          set.remove(&task_id);
        }
      }
    }
    if let Some(dependents) = self.redges.remove(&task_id) {
      for from in dependents {
        if let Some(set) = self.edges.get_mut(&from) {
          set.remove(&task_id);
        }
      }
    }

    self.sub_trie.remove_subscriber(task_id);
    self.notify_subscribers(task_id, handle.path, TaskNotify::Removed);
  }

  fn set_task_label(&mut self, task_id: TaskId, label: Option<String>) {
    let Some(task) = self.tasks.get_mut(&task_id) else {
      return;
    };
    if task.label == label {
      return;
    }
    task.label = label.clone();
    let from_path = task.path.clone();
    self.notify_subscribers(
      task_id,
      from_path,
      TaskNotify::LabelChanged(label),
    );
  }

  fn set_task_path(&mut self, task_id: TaskId, path: TaskPath) {
    if !self.tasks.contains_key(&task_id) {
      return;
    }
    // Reject up front so the task never loses its current path: only free
    // the old one once the new one is known to be available.
    let taken_by_other = self
      .path_trie
      .resolve(&path)
      .is_some_and(|holder| holder != task_id);
    if taken_by_other {
      log::warn!("Path conflict: {} is already taken", path);
      return;
    }
    let old_path = self.tasks.get_mut(&task_id).and_then(|t| t.path.take());
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

  fn handle_query(&self, query: KernelQuery) -> KernelQueryResponse {
    match query {
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
              state: handle.state,
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
        let screen_text = self
          .path_trie
          .resolve(&path)
          .and_then(|task_id| self.tasks.get(&task_id))
          .and_then(|handle| handle.vt.as_ref())
          .and_then(|vt| vt.read().ok())
          .map(|parser| crate::term::ansi::render_screen_ansi(parser.screen()));
        KernelQueryResponse::Screen(screen_text)
      }
      KernelQuery::Explain(path) => {
        let explain = self
          .path_trie
          .resolve(&path)
          .and_then(|id| self.explain(id));
        KernelQueryResponse::Explain(explain)
      }
    }
  }

  fn explain(&self, task_id: TaskId) -> Option<TaskExplain> {
    let task = self.tasks.get(&task_id)?;
    let wanted = self.wanted_set();
    let supported = self.supported_set(&wanted);
    let name = |id: TaskId| {
      self
        .tasks
        .get(&id)
        .and_then(|t| t.path.as_ref())
        .map(|p| p.to_string())
        .unwrap_or_else(|| format!("<task:{}>", id.0))
    };
    let pinned = self
      .redges
      .get(&task_id)
      .is_some_and(|s| s.contains(&INIT_TASK_ID));
    let required_by = self
      .redges
      .get(&task_id)
      .map(|set| {
        set
          .iter()
          .filter(|from| **from != INIT_TASK_ID)
          .map(|from| name(*from))
          .collect()
      })
      .unwrap_or_default();
    let deps = self
      .edges
      .get(&task_id)
      .map(|set| {
        set
          .iter()
          .map(|dep| {
            let task = self.tasks.get(dep);
            DepExplain {
              name: name(*dep),
              state: task.map_or(TaskState::Idle, |t| t.state),
              wanted: wanted.contains(dep),
              satisfied: task.is_some_and(|t| t.is_satisfied()),
            }
          })
          .collect()
      })
      .unwrap_or_default();
    Some(TaskExplain {
      state: task.state,
      wanted: wanted.contains(&task_id),
      supported: supported.contains(&task_id),
      kept_down: task.kept_down,
      pinned,
      required_by,
      deps,
      attempts: task.attempts,
    })
  }

  fn apply_effects(&mut self, task_id: TaskId, fx: &mut Effects) {
    for effect in fx.drain() {
      match effect {
        TaskEffect::Started => self.on_task_started(task_id),
        TaskEffect::Ready => self.on_task_ready(task_id),
        TaskEffect::Stopped(info) => self.on_task_stopped(task_id, info),
      }
    }
  }

  fn notify_subscribers(
    &mut self,
    from: TaskId,
    from_path: Option<TaskPath>,
    notify: TaskNotify,
  ) {
    let mut targets = HashSet::new();
    if let Some(path) = &from_path {
      self.sub_trie.collect(path, &mut targets);
    }
    self.deliver(from, from_path, notify, targets);
  }

  fn notify_path_changed(
    &mut self,
    from: TaskId,
    old: Option<TaskPath>,
    new: Option<TaskPath>,
  ) {
    let (state, label, vt) = match self.tasks.get(&from) {
      Some(t) => (t.state, t.label.clone(), t.vt.clone()),
      None => return,
    };

    let mut old_targets = HashSet::new();
    if let Some(old) = &old {
      self.sub_trie.collect(old, &mut old_targets);
    }
    let mut new_targets = HashSet::new();
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
          state,
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
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use tokio::sync::mpsc::{
    UnboundedReceiver, UnboundedSender, error::TryRecvError, unbounded_channel,
  };

  use super::*;

  #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
  enum RecordedCmd {
    Start,
    Stop,
    Kill,
  }

  /// Test directive delivered via `TaskMsg`, reported back through Effects.
  enum Report {
    Started,
    Ready,
    Stopped(ExitInfo),
  }

  struct RecordingTask {
    name: &'static str,
    tx: UnboundedSender<(&'static str, RecordedCmd)>,
  }

  impl Task for RecordingTask {
    fn handle_cmd(&mut self, cmd: TaskCmd, fx: &mut Effects) {
      match cmd {
        TaskCmd::Start => {
          self.tx.send((self.name, RecordedCmd::Start)).unwrap();
          fx.started();
        }
        TaskCmd::Stop => {
          self.tx.send((self.name, RecordedCmd::Stop)).unwrap();
          fx.stopped(ExitInfo::code(0));
        }
        TaskCmd::Kill => {
          self.tx.send((self.name, RecordedCmd::Kill)).unwrap();
          fx.stopped(ExitInfo::signal(9));
        }
        TaskCmd::Msg(m) => match m.downcast::<Report>() {
          Ok(report) => match *report {
            Report::Started => fx.started(),
            Report::Ready => fx.ready(),
            Report::Stopped(info) => fx.stopped(info),
          },
          Err(_) => (),
        },
      }
    }
  }

  /// Records commands like `RecordingTask` but never reports stopping.
  struct StubbornTask {
    name: &'static str,
    tx: UnboundedSender<(&'static str, RecordedCmd)>,
  }

  impl Task for StubbornTask {
    fn handle_cmd(&mut self, cmd: TaskCmd, fx: &mut Effects) {
      match cmd {
        TaskCmd::Start => {
          self.tx.send((self.name, RecordedCmd::Start)).unwrap();
          fx.started();
        }
        TaskCmd::Stop => {
          self.tx.send((self.name, RecordedCmd::Stop)).unwrap();
        }
        TaskCmd::Kill => {
          self.tx.send((self.name, RecordedCmd::Kill)).unwrap();
        }
        TaskCmd::Msg(_) => (),
      }
    }
  }

  struct Fixture {
    kernel: Option<Kernel>,
    pc: TaskContext,
    rx: UnboundedReceiver<(&'static str, RecordedCmd)>,
    tx: UnboundedSender<(&'static str, RecordedCmd)>,
  }

  impl Fixture {
    fn new() -> Self {
      let kernel = Kernel::new();
      let pc = kernel.context();
      let (tx, rx) = unbounded_channel();
      Self {
        kernel: Some(kernel),
        pc,
        rx,
        tx,
      }
    }

    fn add(&mut self, name: &'static str, def: TaskDef) -> TaskId {
      let tx = self.tx.clone();
      self
        .kernel
        .as_mut()
        .unwrap()
        .register_task(def, move |_| Box::new(RecordingTask { name, tx }))
    }

    fn run(&mut self) -> tokio::task::JoinHandle<()> {
      tokio::spawn(self.kernel.take().unwrap().run())
    }

    async fn recv(&mut self) -> (&'static str, RecordedCmd) {
      tokio::time::timeout(Duration::from_secs(1), self.rx.recv())
        .await
        .expect("timed out waiting for task command")
        .expect("task command channel closed")
    }

    fn assert_no_cmd(&mut self) {
      match self.rx.try_recv() {
        Ok(cmd) => panic!("unexpected task command: {cmd:?}"),
        Err(TryRecvError::Disconnected) => {
          panic!("task command channel closed")
        }
        Err(TryRecvError::Empty) => {}
      }
    }

    /// Round-trip a query so all previously sent messages are processed.
    async fn flush(&self) {
      let rx = self.pc.query(KernelQuery::ListTasks(None));
      tokio::time::timeout(Duration::from_secs(1), rx)
        .await
        .expect("timed out waiting for kernel query response")
        .expect("kernel query response channel closed");
    }

    async fn quit(mut self, handle: tokio::task::JoinHandle<()>) {
      self.pc.send(KernelCommand::Quit);
      // Drain commands so recording sends don't panic on a closed channel.
      let drain =
        tokio::spawn(async move { while self.rx.recv().await.is_some() {} });
      tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("timed out waiting for kernel to quit")
        .unwrap();
      drain.abort();
    }
  }

  fn path_def(path: &str) -> TaskDef {
    TaskDef {
      path: Some(TaskPath::new(path).unwrap()),
      ..Default::default()
    }
  }

  #[tokio::test]
  async fn start_starts_and_down_stops() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Down(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn start_pulls_dependencies_up_in_order() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn registering_pinned_task_starts_it() {
    let mut fx = Fixture::new();
    fx.add(
      "a",
      TaskDef {
        pinned: true,
        ..path_def("/a")
      },
    );
    let handle = fx.run();

    fx.flush().await;
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn keep_down_breaks_dependents_leaf_first() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // Keeping the dep down takes the dependent down first.
    fx.pc.send(KernelCommand::KeepDown(dep));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));

    // The dependent stays wanted but blocked; starting the dep again brings
    // both back.
    fx.pc.send(KernelCommand::Start(dep));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn start_of_dependent_releases_kept_down_dep() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // Keep the dep down: dependent breaks first.
    fx.pc.send(KernelCommand::KeepDown(dep));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));

    // Starting the dependent demands the dep: it is released and both come
    // back, dep first.
    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn start_of_dependent_revives_exited_dep() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // The dep dies on its own (restart: Never => Exited); the dependent
    // breaks and waits.
    fx.pc.send_msg(dep, Report::Stopped(ExitInfo::code(0)));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn start_of_dependent_does_not_rerun_done_job() {
    let mut fx = Fixture::new();
    let job = fx.add(
      "job",
      TaskDef {
        kind: TaskKind::Job,
        ..path_def("/job")
      },
    );
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![job],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("job", RecordedCmd::Start));
    fx.pc.send_msg(job, Report::Stopped(ExitInfo::code(0)));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // Cycling the dependent leaves the completed job alone.
    fx.pc.send(KernelCommand::Restart(app));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn down_keeps_task_wanted_by_another() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    fx.pc.send(KernelCommand::Start(dep));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // Unpinning the dep is a no-op while the app still wants it.
    fx.pc.send(KernelCommand::Down(dep));
    fx.flush().await;
    fx.assert_no_cmd();

    // Unpinning the app winds both down, dependent first.
    fx.pc.send(KernelCommand::Down(app));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn dependent_waits_for_readiness() {
    let mut fx = Fixture::new();
    let dep = fx.add(
      "dep",
      TaskDef {
        ready: ReadyMode::Reported,
        ..path_def("/dep")
      },
    );
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.pc.send_msg(dep, Report::Ready);
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn job_satisfies_dependents_only_when_done() {
    let mut fx = Fixture::new();
    let job = fx.add(
      "job",
      TaskDef {
        kind: TaskKind::Job,
        ..path_def("/job")
      },
    );
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![job],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("job", RecordedCmd::Start));
    fx.flush().await;
    fx.assert_no_cmd();

    // The job completing successfully unblocks the dependent and does not
    // get restarted.
    fx.pc.send_msg(job, Report::Stopped(ExitInfo::code(0)));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn crash_restarts_with_backoff() {
    let mut fx = Fixture::new();
    let a = fx.add(
      "a",
      TaskDef {
        restart: RestartMode::OnFailure,
        ..path_def("/a")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send_msg(a, Report::Stopped(ExitInfo::code(1)));
    // Restarted after the backoff delay.
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn clean_exit_does_not_restart() {
    let mut fx = Fixture::new();
    let a = fx.add(
      "a",
      TaskDef {
        restart: RestartMode::OnFailure,
        ..path_def("/a")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send_msg(a, Report::Stopped(ExitInfo::code(0)));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn restart_cycles_task() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Restart(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    // Restart on a stopped, unpinned task starts it.
    fx.pc.send(KernelCommand::Stop(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));
    fx.pc.send(KernelCommand::Restart(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn stop_of_leaf_keeps_it_down_until_started() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    // Nothing wants the task once the stop unpins it.
    fx.pc.send(KernelCommand::Stop(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn quit_stops_tasks_in_reverse_dependency_order() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Quit);
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));
    tokio::time::timeout(Duration::from_secs(1), handle)
      .await
      .expect("timed out waiting for kernel to quit")
      .unwrap();
  }

  #[tokio::test]
  async fn add_edge_rejects_cycles() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let b = fx.add(
      "b",
      TaskDef {
        deps: vec![a],
        ..path_def("/b")
      },
    );
    let handle = fx.run();

    // a -> b would close the cycle; rejected, so starting b never deadlocks.
    fx.pc.send(KernelCommand::AddEdge { from: a, to: b });
    fx.pc.send(KernelCommand::Start(b));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  async fn label_of(pc: &TaskContext, id: TaskId) -> Option<String> {
    let rx = pc.query(KernelQuery::ListTasks(None));
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
    let mut fx = Fixture::new();
    // The label may hold characters that aren't valid in a path (spaces).
    let id = fx.add(
      "a",
      TaskDef {
        label: Some("web server".to_string()),
        ..path_def("/1")
      },
    );
    let handle = fx.run();

    assert_eq!(label_of(&fx.pc, id).await.as_deref(), Some("web server"));

    fx.pc
      .send(KernelCommand::SetTaskLabel(id, Some("renamed".to_string())));
    assert_eq!(label_of(&fx.pc, id).await.as_deref(), Some("renamed"));

    fx.quit(handle).await;
  }

  async fn state_of(pc: &TaskContext, id: TaskId) -> Option<TaskState> {
    let rx = pc.query(KernelQuery::ListTasks(None));
    let resp = tokio::time::timeout(Duration::from_secs(1), rx)
      .await
      .expect("timed out listing tasks")
      .expect("kernel query channel closed");
    match resp {
      KernelQueryResponse::TaskList(list) => {
        list.into_iter().find(|t| t.id == id).map(|t| t.state)
      }
      _ => panic!("unexpected query response"),
    }
  }

  async fn resolve(pc: &TaskContext, path: &str) -> Option<TaskId> {
    let rx = pc.query(KernelQuery::ResolvePath(TaskPath::new(path).unwrap()));
    let resp = tokio::time::timeout(Duration::from_secs(1), rx)
      .await
      .expect("timed out resolving path")
      .expect("kernel query channel closed");
    match resp {
      KernelQueryResponse::ResolvedPath(id) => id,
      _ => panic!("unexpected query response"),
    }
  }

  #[tokio::test]
  async fn set_task_path_rejects_conflict_and_keeps_old_path() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let b = fx.add("b", path_def("/b"));
    let handle = fx.run();

    // Target taken by another task: rejected, both paths intact.
    fx.pc
      .send(KernelCommand::SetTaskPath(a, TaskPath::new("/b").unwrap()));
    assert_eq!(resolve(&fx.pc, "/a").await, Some(a));
    assert_eq!(resolve(&fx.pc, "/b").await, Some(b));

    // Free target: moves cleanly, old path released.
    fx.pc
      .send(KernelCommand::SetTaskPath(a, TaskPath::new("/c").unwrap()));
    assert_eq!(resolve(&fx.pc, "/c").await, Some(a));
    assert_eq!(resolve(&fx.pc, "/a").await, None);

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn register_path_conflict_keeps_owner() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/x"));
    let b = fx.add("b", path_def("/x"));
    let handle = fx.run();

    // The loser is registered without a path.
    assert_eq!(resolve(&fx.pc, "/x").await, Some(a));

    // Removing the loser must not free the owner's path.
    fx.pc.send(KernelCommand::RemoveTask(b));
    assert_eq!(resolve(&fx.pc, "/x").await, Some(a));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn stale_started_report_is_ignored() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Stop(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));

    // A started report that was in flight when the stop landed must not
    // resurrect the task (or stop it again).
    fx.pc.send_msg(a, Report::Started);
    fx.flush().await;
    fx.assert_no_cmd();

    // The task still starts normally when demanded again.
    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn kill_hard_kills_and_unpins() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    // Kill skips the graceful stop; the unpin keeps the task down.
    fx.pc.send(KernelCommand::Kill(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Kill));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn dep_crash_breaks_dependents_in_order_and_recovers() {
    let mut fx = Fixture::new();
    let c = fx.add(
      "c",
      TaskDef {
        restart: RestartMode::OnFailure,
        ..path_def("/c")
      },
    );
    let b = fx.add(
      "b",
      TaskDef {
        deps: vec![c],
        ..path_def("/b")
      },
    );
    let a = fx.add(
      "a",
      TaskDef {
        deps: vec![b],
        ..path_def("/a")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("c", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    // The crash breaks dependents top-down; after the backoff retry the
    // whole chain returns bottom-up.
    fx.pc.send_msg(c, Report::Stopped(ExitInfo::code(1)));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("c", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn add_edge_to_running_dependent_gates_it() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let dep = fx.add(
      "dep",
      TaskDef {
        ready: ReadyMode::Reported,
        ..path_def("/dep")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    // A new requirement on an unsatisfied dep takes the dependent down
    // until the dep is ready. The stop and the dep start land in the same
    // reconcile pass, so their order is not defined.
    fx.pc.send(KernelCommand::AddEdge { from: a, to: dep });
    let mut cmds = [fx.recv().await, fx.recv().await];
    cmds.sort();
    assert_eq!(
      cmds,
      [("a", RecordedCmd::Stop), ("dep", RecordedCmd::Start)]
    );
    fx.flush().await;
    fx.assert_no_cmd();

    fx.pc.send_msg(dep, Report::Ready);
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn keep_down_of_leaf_dep_tears_down_chain_in_order() {
    let mut fx = Fixture::new();
    let c = fx.add("c", path_def("/c"));
    let b = fx.add(
      "b",
      TaskDef {
        deps: vec![c],
        ..path_def("/b")
      },
    );
    let a = fx.add(
      "a",
      TaskDef {
        deps: vec![b],
        ..path_def("/a")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("c", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    // Keeping the deepest dep down unwinds the chain dependents-first.
    fx.pc.send(KernelCommand::KeepDown(c));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("c", RecordedCmd::Stop));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn stop_of_required_task_bounces_it() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // The app still wants the dep, so the stop is a bounce: the dep is
    // stopped directly, the app breaks and recovers along the way. The
    // middle two land in one reconcile pass, so their order is not defined.
    fx.pc.send(KernelCommand::Stop(dep));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));
    let mut cmds = [fx.recv().await, fx.recv().await];
    cmds.sort();
    assert_eq!(
      cmds,
      [("app", RecordedCmd::Stop), ("dep", RecordedCmd::Start)]
    );
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn restart_of_dep_bounces_it_and_its_dependent() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // The dep is stopped directly; the dependent breaks and recovers once
    // the dep is ready again.
    fx.pc.send(KernelCommand::Restart(dep));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));
    let mut cmds = [fx.recv().await, fx.recv().await];
    cmds.sort();
    assert_eq!(
      cmds,
      [("app", RecordedCmd::Stop), ("dep", RecordedCmd::Start)]
    );
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn restart_pins_like_start() {
    let mut fx = Fixture::new();
    let dep = fx.add("dep", path_def("/dep"));
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Restart(dep));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Stop));
    let mut cmds = [fx.recv().await, fx.recv().await];
    cmds.sort();
    assert_eq!(
      cmds,
      [("app", RecordedCmd::Stop), ("dep", RecordedCmd::Start)]
    );
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Start));

    // The restart pinned the dep, so it survives its dependent going away.
    fx.pc.send(KernelCommand::Down(app));
    assert_eq!(fx.recv().await, ("app", RecordedCmd::Stop));
    fx.flush().await;
    fx.assert_no_cmd();

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn stop_unpins_so_revival_is_temporary() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let b = fx.add(
      "b",
      TaskDef {
        deps: vec![a],
        ..path_def("/b")
      },
    );
    let handle = fx.run();

    // Pin a, then stop it: the stop also unpins.
    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));
    fx.pc.send(KernelCommand::Stop(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));

    // Starting a dependent revives a, but only while b wants it.
    fx.pc.send(KernelCommand::Start(b));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Down(b));
    assert_eq!(fx.recv().await, ("b", RecordedCmd::Stop));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn remove_of_running_task_hard_kills_it() {
    let mut fx = Fixture::new();
    let a = fx.add("a", path_def("/a"));
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send(KernelCommand::RemoveTask(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Kill));
    assert_eq!(state_of(&fx.pc, a).await, None);

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn dead_channel_task_is_marked_exited() {
    use super::super::task::ChannelTask;

    let mut fx = Fixture::new();
    let a = fx
      .kernel
      .as_mut()
      .unwrap()
      .register_task(path_def("/a"), |_| {
        let (tx, rx) = unbounded_channel();
        drop(rx);
        Box::new(ChannelTask::new(tx))
      });
    let handle = fx.run();

    // The driving future is gone; starting must not wedge in Starting.
    fx.pc.send(KernelCommand::Start(a));
    fx.flush().await;
    assert_eq!(
      state_of(&fx.pc, a).await,
      Some(TaskState::Exited(ExitInfo::error()))
    );

    fx.quit(handle).await;
  }

  #[tokio::test(start_paused = true)]
  async fn unresponsive_task_is_killed_then_marked_exited() {
    let mut fx = Fixture::new();
    let tx = fx.tx.clone();
    let a = fx
      .kernel
      .as_mut()
      .unwrap()
      .register_task(path_def("/a"), move |_| {
        Box::new(StubbornTask { name: "a", tx })
      });
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Start));

    fx.pc.send(KernelCommand::Stop(a));
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Stop));
    fx.flush().await;

    // The stop is ignored: after the grace period the kernel hard-kills.
    tokio::time::advance(STOP_GRACE + Duration::from_millis(1)).await;
    assert_eq!(fx.recv().await, ("a", RecordedCmd::Kill));
    fx.flush().await;

    // The kill is also ignored: the kernel gives up so the graph (and
    // quit) can make progress.
    tokio::time::advance(STOP_GRACE + Duration::from_millis(1)).await;
    fx.flush().await;
    assert_eq!(
      state_of(&fx.pc, a).await,
      Some(TaskState::Exited(ExitInfo::error()))
    );

    fx.quit(handle).await;
  }

  #[tokio::test]
  async fn explain_reports_block_reason() {
    let mut fx = Fixture::new();
    let dep = fx.add(
      "dep",
      TaskDef {
        ready: ReadyMode::Reported,
        ..path_def("/dep")
      },
    );
    let app = fx.add(
      "app",
      TaskDef {
        deps: vec![dep],
        ..path_def("/app")
      },
    );
    let handle = fx.run();

    fx.pc.send(KernelCommand::Start(app));
    assert_eq!(fx.recv().await, ("dep", RecordedCmd::Start));

    let rx = fx
      .pc
      .query(KernelQuery::Explain(TaskPath::new("/app").unwrap()));
    let resp = tokio::time::timeout(Duration::from_secs(1), rx)
      .await
      .unwrap()
      .unwrap();
    let explain = match resp {
      KernelQueryResponse::Explain(Some(explain)) => explain,
      _ => panic!("missing explain response"),
    };
    assert_eq!(explain.state, TaskState::Idle);
    assert!(explain.wanted);
    // Wanted but blocked: the dep has not reported ready yet.
    assert!(!explain.supported);
    assert!(explain.pinned);
    assert!(!explain.kept_down);
    assert_eq!(explain.deps.len(), 1);
    assert_eq!(explain.deps[0].name, "/dep");
    assert_eq!(explain.deps[0].state, TaskState::Running);
    assert!(explain.deps[0].wanted);
    assert!(!explain.deps[0].satisfied);

    fx.quit(handle).await;
  }
}
