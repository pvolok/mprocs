use std::{
  fmt::Debug,
  ops::Deref,
  sync::{atomic::AtomicUsize, Arc, RwLock},
};

use tokio::sync::mpsc::UnboundedSender;

use crate::{
  proc::{
    msg::{CustomProcCmd, ProcCmd},
    ReplySender,
  },
  vt100::Parser,
};

use super::proc::{ProcId, ProcInit};

pub struct KernelMessage {
  pub from: ProcId,
  pub command: KernelCommand,
}

pub enum KernelCommand {
  Quit,

  AddProc(ProcId, Box<dyn FnOnce(ProcContext) -> ProcInit + Send>),
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

#[derive(Clone)]
pub struct ProcContext {
  next_proc_id: Arc<AtomicUsize>,
  sender: UnboundedSender<KernelMessage>,
  pub proc_id: ProcId,
}

impl ProcContext {
  pub fn new(
    next_proc_id: Arc<AtomicUsize>,
    proc_id: ProcId,
    sender: UnboundedSender<KernelMessage>,
  ) -> Self {
    Self {
      next_proc_id,
      sender,
      proc_id,
    }
  }

  pub fn send(&self, command: KernelCommand) {
    if let Err(_err) = self.sender.send(KernelMessage {
      from: self.proc_id,
      command,
    }) {
      log::info!(
        "Failed to send kernel message (proc_id: {}). Channel is closed.",
        self.proc_id.0,
      );
    }
  }

  pub fn send_self_custom<T: CustomProcCmd + Send>(&self, custom: T) {
    self.send(KernelCommand::ProcCmd(
      self.proc_id,
      ProcCmd::Custom(Box::new(custom)),
    ));
  }

  pub fn add_proc(
    &self,
    f: Box<dyn FnOnce(ProcContext) -> ProcInit + Send>,
  ) -> ProcId {
    let proc_id = ProcId(
      self
        .next_proc_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    );
    self.send(KernelCommand::AddProc(proc_id, f));
    proc_id
  }

  pub fn get_proc_sender(&self, target_id: ProcId) -> ProcSender {
    ProcSender {
      proc_id: target_id,
      from_id: self.proc_id,
      sender: self.sender.clone(),
    }
  }
}

#[derive(Clone)]
pub struct ProcSender {
  pub proc_id: ProcId,
  pub from_id: ProcId,
  sender: UnboundedSender<KernelMessage>,
}

impl ProcSender {
  pub fn send(&self, cmd: ProcCmd) {
    let r = self.sender.send(KernelMessage {
      from: self.from_id,
      command: KernelCommand::ProcCmd(self.proc_id, cmd),
    });
    if let Err(_err) = r {
      log::debug!(
        "ProcSender.send() to closed channel. from_id:{} proc_id:{}",
        self.from_id.0,
        self.proc_id.0
      );
    }
  }
}
