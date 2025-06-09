use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcId(pub usize);

pub struct ProcHandle2 {
  pub proc_id: ProcId,
  pub sender: UnboundedSender<ProcCommand>,

  pub stop_on_quit: bool,
  pub status: ProcStatus,
}

pub enum ProcStatus {
  Down,
  Running,
}

pub struct ProcInit {
  pub sender: UnboundedSender<ProcCommand>,
  pub stop_on_quit: bool,
  pub status: ProcStatus,
}

pub enum ProcCommand {
  Start,
  Stop,
}

pub struct Proc2 {
  //
}
