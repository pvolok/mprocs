pub struct Proc {
  pub name: String,
}

pub struct State {
  pub procs: Vec<Proc>,
  pub selected: usize,
}
