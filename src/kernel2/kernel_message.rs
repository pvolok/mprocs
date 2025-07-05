use tokio::sync::mpsc::UnboundedSender;

use super::proc::{ProcId, ProcInit};

pub struct KernelMessage2 {
  pub from: ProcId,
  pub command: KernelCommand,
}

pub enum KernelCommand {
  Quit,

  AddProc(Box<dyn FnOnce(KernelSender2) -> ProcInit + Send>),
  StopProc,

  // Proc reporting
  ProcStopped,
}

pub struct KernelSender2 {
  sender: UnboundedSender<KernelMessage2>,
  pub proc_id: ProcId,
}

impl KernelSender2 {
  pub fn new(proc_id: ProcId, sender: UnboundedSender<KernelMessage2>) -> Self {
    Self { sender, proc_id }
  }

  pub fn send(&self, command: KernelCommand) {
    if let Err(_) = self.sender.send(KernelMessage2 {
      from: self.proc_id,
      command,
    }) {
      log::info!(
        "Failed to send kernel message (proc_id: {}). Channel is closed.",
        self.proc_id.0,
      );
    }
  }
}
