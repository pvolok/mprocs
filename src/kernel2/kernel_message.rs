use std::{
  fmt::Debug,
  ops::Deref,
  sync::{atomic::AtomicUsize, Arc, RwLock},
};

use tokio::sync::mpsc::UnboundedSender;

use crate::{
  proc::{msg::ProcCmd, ReplySender},
  vt100::Parser,
};

use super::proc::{ProcId, ProcInit};

pub struct KernelMessage2 {
  pub from: ProcId,
  pub command: KernelCommand,
}

pub enum KernelCommand {
  Quit,

  AddProc(ProcId, Box<dyn FnOnce(KernelSender2) -> ProcInit + Send>),
  ProcCmd(ProcId, ProcCmd),

  ListenProcUpdates,
  UnlistenProcUpdates,

  // Proc reporting
  ProcStarted,
  ProcStopped(u32),
  ProcUpdatedScreen(Option<SharedVt>),
  ProcRendered,
}

#[derive(Clone)]
pub struct SharedVt(Arc<RwLock<Parser<ReplySender>>>);

impl SharedVt {
  pub fn new(parser: Parser<ReplySender>) -> Self {
    SharedVt(Arc::new(RwLock::new(parser)))
  }
}

impl Debug for SharedVt {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("SharedVt").finish()
  }
}

impl Deref for SharedVt {
  type Target = Arc<RwLock<Parser<ReplySender>>>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

pub struct KernelSender2 {
  next_proc_id: Arc<AtomicUsize>,
  sender: UnboundedSender<KernelMessage2>,
  pub proc_id: ProcId,
}

impl KernelSender2 {
  pub fn new(
    next_proc_id: Arc<AtomicUsize>,
    proc_id: ProcId,
    sender: UnboundedSender<KernelMessage2>,
  ) -> Self {
    Self {
      next_proc_id,
      sender,
      proc_id,
    }
  }

  pub fn for_proc(&self, proc_id: ProcId) -> Self {
    Self {
      next_proc_id: self.next_proc_id.clone(),
      sender: self.sender.clone(),
      proc_id,
    }
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

  pub fn add_proc(
    &self,
    f: Box<dyn FnOnce(KernelSender2) -> ProcInit + Send>,
  ) -> KernelSender2 {
    let proc_id = ProcId(
      self
        .next_proc_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    );
    self.send(KernelCommand::AddProc(proc_id, f));
    self.for_proc(proc_id)
  }
}
