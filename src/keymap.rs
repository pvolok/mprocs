use std::collections::HashMap;

use crate::{event::AppEvent, key::Key, state::Scope};

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

  pub fn resolve(&self, scope: Scope, key: &Key) -> Option<&AppEvent> {
    let map = match scope {
      Scope::Procs => &self.procs,
      Scope::Term | Scope::TermZoom => &self.term,
    };
    map.get(key)
  }
}
