use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{event::AppEvent, state::Scope};

pub struct Keymap {
  pub procs: HashMap<KeyEvent, AppEvent>,
  pub term: HashMap<KeyEvent, AppEvent>,
}

impl Keymap {
  pub fn new() -> Self {
    Keymap {
      procs: HashMap::new(),
      term: HashMap::new(),
    }
  }

  pub fn bind_p(&mut self, key: KeyEvent, event: AppEvent) {
    self.procs.insert(key, event);
  }

  pub fn bind_t(&mut self, key: KeyEvent, event: AppEvent) {
    self.term.insert(key, event);
  }

  pub fn resolve(&self, scope: Scope, key: &KeyEvent) -> Option<&AppEvent> {
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
      KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleScope,
    );
    keymap.bind_t(
      KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleScope,
    );

    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
      AppEvent::Quit,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT),
      AppEvent::ForceQuit,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
      AppEvent::NextProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
      AppEvent::NextProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
      AppEvent::PrevProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
      AppEvent::PrevProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
      AppEvent::StartProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
      AppEvent::TermProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
      AppEvent::KillProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
      AppEvent::RestartProc,
    );
    keymap.bind_p(
      KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT),
      AppEvent::ForceRestartProc,
    );
    let ctrlc = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    keymap.bind_p(ctrlc, AppEvent::SendKey(ctrlc));

    let ctrlu = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
    keymap.bind_p(ctrlu, AppEvent::ScrollUp);
    keymap.bind_p(
      KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
      AppEvent::ScrollUp,
    );
    let ctrld = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
    keymap.bind_p(ctrld, AppEvent::ScrollDown);
    keymap.bind_p(
      KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
      AppEvent::ScrollDown,
    );

    keymap
  }
}
