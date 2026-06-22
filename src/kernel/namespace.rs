use std::collections::HashSet;

use super::{
  path_trie::{PathConflictError, PathTrie},
  sub_trie::{SubMode, SubTrie},
  task::TaskId,
  task_path::TaskPath,
};

/// Naming and pub/sub for tasks: resolves paths to ids and routes
/// notifications to subscribers. Bundles the path index and the subscription
/// trie behind one façade so the graph deals with a single collaborator for
/// everything name- and subscription-related.
pub struct Namespace {
  paths: PathTrie,
  subs: SubTrie,
}

impl Namespace {
  pub fn new() -> Self {
    Self {
      paths: PathTrie::new(),
      subs: SubTrie::new(),
    }
  }

  // ---- Naming ----

  pub fn insert(
    &mut self,
    path: &TaskPath,
    id: TaskId,
  ) -> Result<(), PathConflictError> {
    self.paths.insert(path, id)
  }

  pub fn remove(&mut self, path: &TaskPath) -> Option<TaskId> {
    self.paths.remove(path)
  }

  pub fn resolve(&self, path: &TaskPath) -> Option<TaskId> {
    self.paths.resolve(path)
  }

  pub fn glob(&self, pattern: &str) -> Vec<(TaskPath, TaskId)> {
    self.paths.glob(pattern)
  }

  pub fn iter(&self) -> Vec<(TaskPath, TaskId)> {
    self.paths.iter()
  }

  // ---- Pub/sub ----

  pub fn subscribe(
    &mut self,
    subscriber: TaskId,
    path: &TaskPath,
    mode: SubMode,
  ) {
    self.subs.subscribe(subscriber, path, mode);
  }

  pub fn unsubscribe(
    &mut self,
    subscriber: TaskId,
    path: &TaskPath,
    mode: SubMode,
  ) {
    self.subs.unsubscribe(subscriber, path, mode);
  }

  pub fn remove_subscriber(&mut self, subscriber: TaskId) {
    self.subs.remove_subscriber(subscriber);
  }

  pub fn collect(&self, path: &TaskPath, out: &mut HashSet<TaskId>) {
    self.subs.collect(path, out);
  }
}
