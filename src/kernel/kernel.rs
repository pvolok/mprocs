use std::{
  collections::{HashMap, HashSet},
  sync::{atomic::AtomicUsize, Arc},
};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
  error::ResultLogger,
  kernel::{kernel_message::ProcContext, proc::DepInfo},
  proc::msg::{ProcCmd, ProcUpdate},
};

use super::{
  kernel_message::{KernelCommand, KernelMessage},
  proc::{ProcHandle, ProcId, ProcInit, ProcStatus},
};

pub struct Kernel {
  sender: UnboundedSender<KernelMessage>,
  receiver: UnboundedReceiver<KernelMessage>,

  quitting: bool,
  next_proc_id: Arc<AtomicUsize>,
  procs: HashMap<ProcId, ProcHandle>,
  /// If `a` requires `b`, then `rev_deps = {b: [a]}`.
  rev_deps: HashMap<ProcId, HashSet<ProcId>>,
  listeners: HashSet<ProcId>,
}

impl Kernel {
  pub fn new() -> Self {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    Self {
      sender,
      receiver,

      quitting: false,
      next_proc_id: Arc::new(AtomicUsize::new(1)),
      procs: HashMap::new(),
      rev_deps: HashMap::new(),
      listeners: Default::default(),
    }
  }

  pub fn spawn_proc<F>(&mut self, f: F) -> ProcId
  where
    F: FnOnce(ProcContext) -> ProcInit,
  {
    let proc_id = ProcId(
      self
        .next_proc_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    );
    self.spawn_proc_with_id(proc_id, f);
    proc_id
  }

  pub fn spawn_proc_with_id<F>(&mut self, proc_id: ProcId, f: F)
  where
    F: FnOnce(ProcContext) -> ProcInit,
  {
    let kernel_sender =
      ProcContext::new(self.next_proc_id.clone(), proc_id, self.sender.clone());
    let init = f(kernel_sender);
    let mut proc_handle = ProcHandle {
      proc_id,
      sender: init.sender,

      stop_on_quit: init.stop_on_quit,
      status: init.status,
      waiting_deps: false,

      deps: HashMap::new(),
    };

    for dep_id in &init.deps {
      proc_handle.deps.insert(
        *dep_id,
        DepInfo {
          status: self
            .procs
            .get(dep_id)
            .map_or(ProcStatus::Down, |d| d.status),
        },
      );
      self.rev_deps.entry(*dep_id).or_default().insert(proc_id);
    }

    self.procs.insert(proc_id, proc_handle);
  }

  pub async fn run(mut self) {
    loop {
      let msg = if let Some(msg) = self.receiver.recv().await {
        msg
      } else {
        log::warn!("Kernel receiver returned None.");
        break;
      };

      match msg.command {
        KernelCommand::Quit => {
          if self.quitting {
            break;
          }
          self.quitting = true;

          for proc in self.procs.values() {
            let _ = proc.sender.send(ProcCmd::Stop);
          }

          if self.is_ready_to_quit() {
            break;
          }
        }

        KernelCommand::AddProc(proc_id, create_proc) => {
          self.spawn_proc_with_id(proc_id, create_proc);
        }
        KernelCommand::ProcCmd(proc_id, cmd) => {
          if let Some(proc) = self.procs.get(&proc_id) {
            match cmd {
              ProcCmd::Start => {
                let all_deps_ready = proc
                  .deps
                  .iter()
                  .all(|(_, dep)| dep.status == ProcStatus::Running);
                if all_deps_ready {
                  proc.send(cmd);
                }
              }
              ProcCmd::Stop | ProcCmd::Kill => {
                proc.send(cmd);
              }
              ProcCmd::SendKey(_)
              | ProcCmd::SendMouse(_)
              | ProcCmd::ScrollUp
              | ProcCmd::ScrollDown
              | ProcCmd::ScrollUpLines { .. }
              | ProcCmd::ScrollDownLines { .. }
              | ProcCmd::Resize { .. }
              | ProcCmd::Custom(_)
              | ProcCmd::OnProcUpdate(_, _) => {
                proc.send(cmd);
              }
            }
          }
        }

        KernelCommand::ProcStarted => {
          // Went from DOWN to UP.
          let mut started = false;
          if let Some(proc) = self.procs.get_mut(&msg.from) {
            match proc.status {
              ProcStatus::Down => {
                started = true;
              }
              ProcStatus::Running => (),
            }
            proc.status = ProcStatus::Running;
          }

          if started {
            if let Some(rev_deps) = self.rev_deps.get(&msg.from) {
              for rev_dep_id in rev_deps {
                if let Some(rev_dep) = self.procs.get_mut(rev_dep_id) {
                  let mut all_deps_ready = true;
                  for (dep_id, dep) in &mut rev_dep.deps {
                    if *dep_id == msg.from {
                      dep.status = ProcStatus::Running;
                    }
                    if dep.status != ProcStatus::Running {
                      all_deps_ready = false;
                    }
                  }
                  if all_deps_ready {
                    self
                      .sender
                      .send(KernelMessage {
                        from: ProcId(0),
                        command: KernelCommand::ProcCmd(
                          *rev_dep_id,
                          ProcCmd::Start,
                        ),
                      })
                      .log_ignore();
                  }
                }
              }
            }
          }

          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.procs.get(listener_id) {
              listener
                .send(ProcCmd::OnProcUpdate(msg.from, ProcUpdate::Started));
            }
          }
        }
        KernelCommand::ProcStopped(exit_code) => {
          if let Some(proc) = self.procs.get_mut(&msg.from) {
            proc.status = ProcStatus::Down;
          }

          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.procs.get(listener_id) {
              listener.send(ProcCmd::OnProcUpdate(
                msg.from,
                ProcUpdate::Stopped(exit_code),
              ));
            }
          }

          if self.quitting && self.is_ready_to_quit() {
            break;
          }
        }
        KernelCommand::ProcUpdatedScreen(vt) => {
          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.procs.get(listener_id) {
              listener.send(ProcCmd::OnProcUpdate(
                msg.from,
                ProcUpdate::ScreenChanged(vt.clone()),
              ));
            }
          }
        }
        KernelCommand::ProcRendered => {
          for listener_id in self.listeners.iter() {
            if let Some(listener) = self.procs.get(listener_id) {
              listener
                .send(ProcCmd::OnProcUpdate(msg.from, ProcUpdate::Rendered));
            }
          }
        }

        KernelCommand::ListenProcUpdates => {
          self.listeners.insert(msg.from);
        }
        KernelCommand::UnlistenProcUpdates => {
          self.listeners.remove(&msg.from);
        }
      }
    }
    log::debug!("After kernel loop.");
  }

  fn is_ready_to_quit(&self) -> bool {
    for proc in self.procs.values() {
      match proc.status {
        ProcStatus::Running if proc.stop_on_quit => return false,
        _ => (),
      }
    }
    true
  }
}
