use tui_input::Input;

use crate::proc::Proc;

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
  pub scope: Scope,
  pub procs: Vec<Proc>,
  pub selected: usize,

  pub modal: Option<Modal>,

  pub quitting: bool,
}

impl State {
  pub fn get_current_proc(&self) -> Option<&Proc> {
    self.procs.get(self.selected)
  }

  pub fn get_current_proc_mut(&mut self) -> Option<&mut Proc> {
    self.procs.get_mut(self.selected)
  }

  pub fn get_proc_mut(&mut self, id: usize) -> Option<&mut Proc> {
    self.procs.iter_mut().find(|proc| proc.id == id)
  }

  pub fn all_procs_down(&self) -> bool {
    self.procs.iter().all(|proc| !proc.is_up())
  }
}

pub enum Modal {
  AddProc { input: Input },
  RemoveProc { id: usize },
}
