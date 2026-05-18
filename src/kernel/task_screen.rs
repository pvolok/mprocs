use compact_str::CompactString;

use crate::{
  kernel::{
    kernel_message::{SharedVt, TaskSender},
    task::{TaskCmd, TaskId},
  },
  term::{VtEvent, Winsize},
};

pub struct TaskScreen {
  task_id: TaskId,
  size: Winsize,
  vt: SharedVt,
  // Per read events buffer. It is cleared in the beginning of each process().
  events_buf: Vec<VtEvent>,

  observers: Vec<TaskScreenObs>,
}

pub struct TaskScreenObs {
  kind: ScreenObsKind,
  size: Winsize,
  sender: TaskSender,
}

pub enum ScreenObsKind {
  FrameDiff,
  Direct,
}

pub enum TaskScreenCmd {
  Observe { size: Winsize, sender: TaskSender },
  Unobserve { observer_id: TaskId },
  Resize { size: Winsize, observer_id: TaskId },
}

pub enum FramedScreenNotify {
  ObserveStarted { task_id: TaskId },
  Render { task_id: TaskId },

  Bell { task_id: TaskId },
}

pub enum TaskScreenEffect {
  Reply(CompactString),
  Resize(Winsize),
}

pub enum DirectScreenNotify {
  Print(bytes::Bytes),
}

impl TaskScreen {
  pub fn vt(&self) -> &SharedVt {
    &self.vt
  }

  pub fn new(task_id: TaskId, vt: SharedVt) -> Self {
    let size = vt.read().unwrap().screen().size();
    TaskScreen {
      task_id,
      size: Winsize {
        x: size.width,
        y: size.height,
        x_px: 0,
        y_px: 0,
      },
      vt,
      events_buf: Vec::new(),
      observers: Vec::new(),
    }
  }

  pub fn process(&mut self, bytes: &[u8], effects: &mut Vec<TaskScreenEffect>) {
    let bytes = bytes::Bytes::copy_from_slice(bytes);

    if let Ok(mut vt) = self.vt.write() {
      vt.screen.process(&bytes, &mut self.events_buf);
    }

    for obs in &mut self.observers {
      match obs.kind {
        ScreenObsKind::FrameDiff => {
          for event in &self.events_buf {
            match event {
              VtEvent::Bell => {
                obs.sender.send(TaskCmd::msg(FramedScreenNotify::Bell {
                  task_id: self.task_id,
                }));
              }
              VtEvent::Reply(_) => (),
            }
          }
          obs.sender.send(TaskCmd::msg(FramedScreenNotify::Render {
            task_id: self.task_id,
          }));
        }
        ScreenObsKind::Direct => {
          obs
            .sender
            .send(TaskCmd::msg(DirectScreenNotify::Print(bytes.clone())));
        }
      }
    }

    for event in self.events_buf.drain(..) {
      match event {
        VtEvent::Bell => (),
        VtEvent::Reply(s) => {
          effects.push(TaskScreenEffect::Reply(s));
        }
      }
    }
  }

  pub fn handle_cmd(
    &mut self,
    cmd: TaskScreenCmd,
    effects: &mut Vec<TaskScreenEffect>,
  ) {
    match cmd {
      TaskScreenCmd::Observe { size, sender } => {
        sender.send(TaskCmd::msg(FramedScreenNotify::ObserveStarted {
          task_id: self.task_id,
        }));
        self.observers.push(TaskScreenObs {
          kind: ScreenObsKind::FrameDiff,
          size,
          sender,
        });
        self.sync_size(effects);
      }
      TaskScreenCmd::Unobserve { observer_id } => {
        self.observers.retain(|o| o.sender.task_id != observer_id);
        self.sync_size(effects);
      }
      TaskScreenCmd::Resize { size, observer_id } => {
        if let Some(observer) = self
          .observers
          .iter_mut()
          .find(|o| o.sender.task_id == observer_id)
        {
          observer.size = size;
        }
        self.sync_size(effects);
      }
    }
  }

  pub fn notify_render(&mut self) {
    for obs in &mut self.observers {
      match obs.kind {
        ScreenObsKind::FrameDiff => {
          obs.sender.send(TaskCmd::msg(FramedScreenNotify::Render {
            task_id: self.task_id,
          }));
        }
        ScreenObsKind::Direct => {}
      }
    }
  }

  pub fn sync_size(&mut self, effects: &mut Vec<TaskScreenEffect>) {
    let mut size = self.size;
    if let Some(observer) = self.observers.first() {
      size = observer.size;
    }
    if size != self.size {
      self.size = size;
      effects.push(TaskScreenEffect::Resize(size));
    }
  }
}
