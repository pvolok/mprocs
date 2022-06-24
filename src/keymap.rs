use std::collections::HashMap;

use crate::{event::AppEvent, key::Key, state::Scope};

#[derive(Debug)]
pub struct Keymap {
  pub procs: HashMap<Key, AppEvent>,
  pub term: HashMap<Key, AppEvent>,
}

impl Keymap {
  pub fn new() -> Self {
    Keymap {
      procs: HashMap::new(),
      term: HashMap::new(),
    }
  }

  pub fn bind_p(&mut self, key: Key, event: AppEvent) {
    self.procs.insert(key, event);
  }

  pub fn bind_t(&mut self, key: Key, event: AppEvent) {
    self.term.insert(key, event);
  }

  pub fn resolve_key(&self, scope: Scope, app_event: &AppEvent) -> Vec<&Key> {
    let map = match scope {
      Scope::Procs => &self.procs,
      Scope::Term | Scope::TermZoom => &self.term,
    };
    let mut vec = Vec::new();
    for (k, v) in map.iter() {
      if v == app_event {
        vec.push(k)
      }
    }
    vec
  }

  /// Returns non default key if present
  pub fn non_default_key(
    &self,
    scope: Scope,
    app_event: &AppEvent,
    default_keys: Vec<&Key>,
  ) -> Option<&Key> {
    let keys = self.resolve_key(scope, app_event);
    keys
      .into_iter()
      .filter(|key| !default_keys.contains(key))
      .next()
  }

  pub fn resolve(&self, scope: Scope, key: &Key) -> Option<&AppEvent> {
    let map = match scope {
      Scope::Procs => &self.procs,
      Scope::Term | Scope::TermZoom => &self.term,
    };
    map.get(key)
  }
}
