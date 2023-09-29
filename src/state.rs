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

  pub quitting: bool,
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
}
