use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::{error::ResultLogger, proc::msg::ProcCmd};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProcId(pub usize);

pub struct ProcHandle2 {
  pub proc_id: ProcId,
  pub sender: UnboundedSender<ProcCmd>,

  pub stop_on_quit: bool,
  pub status: ProcStatus,
}

impl ProcHandle2 {
  pub fn send(&self, cmd: ProcCmd) {
    self.sender.send(cmd).log_ignore();
  }
}

pub enum ProcStatus {
  Down,
  Running,
}

pub struct ProcInit {
  pub sender: UnboundedSender<ProcCmd>,
  pub stop_on_quit: bool,
  pub status: ProcStatus,
}
