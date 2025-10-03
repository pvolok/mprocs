use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::{error::ResultLogger, proc::msg::ProcCmd};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProcId(pub usize);

pub struct ProcHandle {
  pub proc_id: ProcId,
  pub sender: UnboundedSender<ProcCmd>,

  pub stop_on_quit: bool,
  pub status: ProcStatus,
  pub waiting_deps: bool,

  pub deps: HashMap<ProcId, DepInfo>,
}

impl ProcHandle {
  pub fn send(&self, cmd: ProcCmd) {
    self.sender.send(cmd).log_ignore();
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProcStatus {
  Down,
  Running,
}

pub struct ProcInit {
  pub sender: UnboundedSender<ProcCmd>,
  pub stop_on_quit: bool,
  pub status: ProcStatus,
  pub deps: Vec<ProcId>,
}

pub struct DepInfo {
  pub status: ProcStatus,
}
