use std::{collections::HashMap, time::Duration};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::kernel2::kernel_message::KernelSender2;

use super::{
  kernel_message::{KernelCommand, KernelMessage2},
  proc::{ProcCommand, ProcHandle2, ProcId, ProcInit, ProcStatus},
};

pub struct Kernel2 {
  sender: UnboundedSender<KernelMessage2>,
  receiver: UnboundedReceiver<KernelMessage2>,

  quitting: bool,
  last_proc_id: usize,
  procs: HashMap<ProcId, ProcHandle2>,
}

impl Kernel2 {
  pub fn new() -> Self {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    Self {
      sender,
      receiver,

      quitting: false,
      last_proc_id: 0,
      procs: HashMap::new(),
    }
  }

  pub fn spawn_proc<F>(&mut self, f: F) -> ProcId
  where
    F: FnOnce(KernelSender2) -> ProcInit,
  {
    self.last_proc_id += 1;
    let proc_id = ProcId(self.last_proc_id);
    let kernel_sender = KernelSender2::new(proc_id, self.sender.clone());
    let init = f(kernel_sender);
    let proc_handle = ProcHandle2 {
      proc_id,
      sender: init.sender,

      stop_on_quit: init.stop_on_quit,
      status: init.status,
    };
    self.procs.insert(proc_id, proc_handle);

    proc_id
  }

  pub async fn run(mut self) {
    let _ = self.sender.send(KernelMessage2 {
      from: ProcId(0),
      command: KernelCommand::AddProc(Box::new(|kernel_sender| {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
          tokio::time::sleep(Duration::from_secs(3)).await;
          kernel_sender.send(KernelCommand::ProcStopped);
        });

        ProcInit {
          sender,
          stop_on_quit: true,
          status: ProcStatus::Running,
        }
      })),
    });

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
            let _ = proc.sender.send(ProcCommand::Stop);
          }

          if self.is_ready_to_quit() {
            break;
          }
        }

        KernelCommand::AddProc(create_proc) => {
          self.spawn_proc(create_proc);
        }
        KernelCommand::StopProc => todo!(),

        KernelCommand::ProcStopped => {
          if let Some(proc) = self.procs.get_mut(&msg.from) {
            proc.status = ProcStatus::Down;
          }
          if self.quitting && self.is_ready_to_quit() {
            break;
          }
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
