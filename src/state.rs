use crate::{
  app::ClientId,
  keymap::KeymapGroup,
  proc::{handle::ProcHandle, CopyMode},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
  Procs,
  Term,
  TermZoom,
}

impl Scope {
  pub fn toggle(&self) -> Self {
    match self {
      Scope::Procs => Scope::Term,
      Scope::Term => Scope::Procs,
      Scope::TermZoom => Scope::Procs,
    }
  }

  pub fn is_zoomed(&self) -> bool {
    match self {
      Scope::Procs => false,
      Scope::Term => false,
      Scope::TermZoom => true,
    }
  }
}

pub struct State {
  pub current_client_id: Option<ClientId>,

  pub scope: Scope,
  pub procs: Vec<ProcHandle>,
  pub selected: usize,
  pub hide_keymap_window: bool,

  pub quitting: bool,
  
  // Search state
  pub search_active: bool,
  pub search_query: String,
  pub search_matches: Vec<(usize, usize)>, // (line, column) positions
  pub current_match: Option<usize>,
}

impl State {
  pub fn get_current_proc(&self) -> Option<&ProcHandle> {
    self.procs.get(self.selected)
  }

  pub fn get_current_proc_mut(&mut self) -> Option<&mut ProcHandle> {
    self.procs.get_mut(self.selected)
  }

  pub fn select_proc(&mut self, index: usize) {
    self.selected = index;
    if let Some(proc_handle) = self.procs.get_mut(index) {
      proc_handle.focus();
    }
  }

  pub fn get_proc_mut(&mut self, id: usize) -> Option<&mut ProcHandle> {
    self.procs.iter_mut().find(|p| p.id() == id)
  }

  pub fn get_keymap_group(&self) -> KeymapGroup {
    match self.scope {
      Scope::Procs => KeymapGroup::Procs,
      Scope::Term | Scope::TermZoom => match self.get_current_proc() {
        Some(proc) => match proc.copy_mode() {
          CopyMode::None(_) => KeymapGroup::Term,
          CopyMode::Start(_, _) | CopyMode::Range(_, _, _) => KeymapGroup::Copy,
        },
        None => KeymapGroup::Term,
      },
    }
  }

  pub fn all_procs_down(&self) -> bool {
    self.procs.iter().all(|p| !p.is_up())
  }

  pub fn toggle_keymap_window(&mut self) {
    self.hide_keymap_window = !self.hide_keymap_window;
  }

  pub fn start_search(&mut self) {
    self.search_active = true;
    self.search_query.clear();
    self.search_matches.clear();
    self.current_match = None;
  }

  pub fn cancel_search(&mut self) {
    self.search_active = false;
    self.search_query.clear();
    self.search_matches.clear();
    self.current_match = None;
  }

  pub fn update_search(&mut self, query: &str) {
    self.search_query = query.to_string();
    // Search matches will be updated in ui_term.rs
  }

  pub fn next_match(&mut self) {
    if let Some(current) = self.current_match {
      if current + 1 < self.search_matches.len() {
        self.current_match = Some(current + 1);
      } else {
        self.current_match = Some(0); // Wrap around
      }
    } else if !self.search_matches.is_empty() {
      self.current_match = Some(0);
    }
  }

  pub fn prev_match(&mut self) {
    if let Some(current) = self.current_match {
      if current > 0 {
        self.current_match = Some(current - 1);
      } else {
        self.current_match = Some(self.search_matches.len() - 1); // Wrap around
      }
    } else if !self.search_matches.is_empty() {
      self.current_match = Some(self.search_matches.len() - 1);
    }
  }
}

impl Default for State {
  fn default() -> Self {
    Self {
      current_client_id: None,
      scope: Scope::Procs,
      procs: Vec::new(),
      selected: 0,
      hide_keymap_window: false,
      quitting: false,
      search_active: false,
      search_query: String::new(),
      search_matches: Vec::new(),
      current_match: None,
    }
  }
}
