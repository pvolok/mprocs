use std::collections::HashMap;

use crate::console::action::Action;
use crate::term::key::Key;

pub struct Keymap {
  pub procs: HashMap<Key, Action>,
  pub rev_procs: HashMap<Action, Key>,
  pub term: HashMap<Key, Action>,
  pub rev_term: HashMap<Action, Key>,
  pub copy: HashMap<Key, Action>,
  pub rev_copy: HashMap<Action, Key>,
}

#[derive(Clone, Copy, Debug)]
pub enum KeymapGroup {
  Procs,
  Term,
  Copy,
}

impl Keymap {
  pub fn new() -> Self {
    Keymap {
      procs: HashMap::new(),
      rev_procs: HashMap::new(),
      term: HashMap::new(),
      rev_term: HashMap::new(),
      copy: HashMap::new(),
      rev_copy: HashMap::new(),
    }
  }

  pub fn bind(&mut self, group: KeymapGroup, key: Key, event: Action) {
    let (map, rev_map) = match group {
      KeymapGroup::Procs => (&mut self.procs, &mut self.rev_procs),
      KeymapGroup::Term => (&mut self.term, &mut self.rev_term),
      KeymapGroup::Copy => (&mut self.copy, &mut self.rev_copy),
    };
    map.insert(key, event.clone());
    rev_map.insert(event, key);
  }

  pub fn bind_p(&mut self, key: Key, event: Action) {
    self.bind(KeymapGroup::Procs, key, event);
  }

  pub fn bind_t(&mut self, key: Key, event: Action) {
    self.bind(KeymapGroup::Term, key, event);
  }

  pub fn bind_c(&mut self, key: Key, event: Action) {
    self.bind(KeymapGroup::Copy, key, event);
  }

  pub fn resolve(&self, group: KeymapGroup, key: &Key) -> Option<&Action> {
    let map = match group {
      KeymapGroup::Procs => &self.procs,
      KeymapGroup::Term => &self.term,
      KeymapGroup::Copy => &self.copy,
    };
    map.get(key)
  }

  pub fn resolve_key(
    &self,
    group: KeymapGroup,
    event: &Action,
  ) -> Option<&Key> {
    let rev_map = match group {
      KeymapGroup::Procs => &self.rev_procs,
      KeymapGroup::Term => &self.rev_term,
      KeymapGroup::Copy => &self.rev_copy,
    };
    rev_map.get(event)
  }
}
