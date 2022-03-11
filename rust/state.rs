use crate::proc::Proc;

pub struct State {
  pub procs: Vec<Proc>,
  pub selected: usize,
}

impl State {
  pub fn get_current_proc(&self) -> Option<&Proc> {
    self.procs.get(self.selected)
  }
}
