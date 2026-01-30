use crate::{
  app::ClientId,
  kernel::proc::ProcId,
  keymap::KeymapGroup,
  proc::{view::ProcView, CopyMode},
  widgets::list::ListState,
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
  pub procs: Vec<ProcView>,
  pub procs_list: ListState,
  pub hide_keymap_window: bool,

  pub quitting: bool,
}

impl State {
  pub fn selected(&self) -> usize {
    self.procs_list.selected()
  }

  pub fn get_current_proc(&self) -> Option<&ProcView> {
    self.procs.get(self.procs_list.selected())
  }

  pub fn get_current_proc_mut(&mut self) -> Option<&mut ProcView> {
    self.procs.get_mut(self.procs_list.selected())
  }

  pub fn select_proc(&mut self, index: usize) {
    self.procs_list.select(index);
    if let Some(proc_handle) = self.procs.get_mut(index) {
      proc_handle.focus();
    }
  }

  pub fn get_proc_mut(&mut self, id: ProcId) -> Option<&mut ProcView> {
    self.procs.iter_mut().find(|p| p.id() == id)
  }

  pub fn get_keymap_group(&self) -> KeymapGroup {
    match self.scope {
      Scope::Procs => KeymapGroup::Procs,
      Scope::Term | Scope::TermZoom => match self.get_current_proc() {
        Some(proc) => match proc.copy_mode() {
          CopyMode::None(_) => KeymapGroup::Term,
          CopyMode::Active(_, _, _) => KeymapGroup::Copy,
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
}
