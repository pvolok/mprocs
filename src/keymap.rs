use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};

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
      Scope::Term => &self.term,
    };
    map.get(key)
  }
}

impl Default for Keymap {
  fn default() -> Self {
    let mut keymap = Self::new();

    keymap.bind_p(
      Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleScope,
    );
    keymap.bind_t(
      Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleScope,
    );

    keymap.bind_p(
      Key::new(KeyCode::Char('q'), KeyModifiers::NONE),
      AppEvent::Quit,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('Q'), KeyModifiers::SHIFT),
      AppEvent::ForceQuit,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('j'), KeyModifiers::NONE),
      AppEvent::NextProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Down, KeyModifiers::NONE),
      AppEvent::NextProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('k'), KeyModifiers::NONE),
      AppEvent::PrevProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Up, KeyModifiers::NONE),
      AppEvent::PrevProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('s'), KeyModifiers::NONE),
      AppEvent::StartProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('x'), KeyModifiers::NONE),
      AppEvent::TermProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
      AppEvent::KillProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('r'), KeyModifiers::NONE),
      AppEvent::RestartProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('R'), KeyModifiers::SHIFT),
      AppEvent::ForceRestartProc,
    );
    let ctrlc = Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    keymap.bind_p(ctrlc, AppEvent::SendKey { key: ctrlc });
    keymap.bind_p(
      Key::new(KeyCode::Char('a'), KeyModifiers::NONE),
      AppEvent::ShowAddProc,
    );
    keymap.bind_p(
      Key::new(KeyCode::Char('d'), KeyModifiers::NONE),
      AppEvent::ShowRemoveProc,
    );

    let ctrlu = Key::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
    keymap.bind_p(ctrlu, AppEvent::ScrollUp);
    keymap.bind_p(
      Key::new(KeyCode::PageUp, KeyModifiers::NONE),
      AppEvent::ScrollUp,
    );
    let ctrld = Key::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
    keymap.bind_p(ctrld, AppEvent::ScrollDown);
    keymap.bind_p(
      Key::new(KeyCode::PageDown, KeyModifiers::NONE),
      AppEvent::ScrollDown,
    );

    keymap
  }
}
