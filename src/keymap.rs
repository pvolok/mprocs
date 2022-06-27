use std::collections::HashMap;

use crate::{event::AppEvent, key::Key, state::Scope};

pub struct Keymap {
  pub procs: HashMap<Key, AppEvent>,
  pub rev_procs: HashMap<AppEvent, Key>,
  pub term: HashMap<Key, AppEvent>,
  pub rev_term: HashMap<AppEvent, Key>,
}

pub enum KeymapGroup {
  Procs,
  Term,
}

impl Keymap {
  pub fn new() -> Self {
    Keymap {
      procs: HashMap::new(),
      rev_procs: HashMap::new(),
      term: HashMap::new(),
      rev_term: HashMap::new(),
    }
  }

  pub fn bind(&mut self, group: KeymapGroup, key: Key, event: AppEvent) {
    let (map, rev_map) = match group {
      KeymapGroup::Procs => (&mut self.procs, &mut self.rev_procs),
      KeymapGroup::Term => (&mut self.term, &mut self.rev_term),
    };
    map.insert(key.clone(), event.clone());
    rev_map.insert(event, key);
  }

  pub fn bind_p(&mut self, key: Key, event: AppEvent) {
    self.bind(KeymapGroup::Procs, key, event);
  }

  pub fn bind_t(&mut self, key: Key, event: AppEvent) {
    self.bind(KeymapGroup::Term, key, event);
  }

  pub fn resolve(&self, scope: Scope, key: &Key) -> Option<&AppEvent> {
    let map = match scope {
      Scope::Procs => &self.procs,
      Scope::Term | Scope::TermZoom => &self.term,
    };
    map.get(key)
  }

  pub fn resolve_key(&self, scope: Scope, event: &AppEvent) -> Option<&Key> {
    let rev_map = match scope {
      Scope::Procs => &self.rev_procs,
      Scope::Term | Scope::TermZoom => &self.rev_term,
    };
    rev_map.get(event)
  }
}
