use std::{any::Any, fmt::Debug};

use crate::{
  kernel::{kernel_message::SharedVt, proc::ProcId},
  key::Key,
  mouse::MouseEvent,
};

#[derive(Debug)]
pub enum ProcCmd {
  Start,
  Stop,
  Kill,

  SendKey(Key),
  SendMouse(MouseEvent),

  ScrollUp,
  ScrollDown,
  ScrollUpLines { n: usize },
  ScrollDownLines { n: usize },

  Resize { w: u16, h: u16 },

  Custom(Box<dyn CustomProcCmd + Send + 'static>),

  OnProcUpdate(ProcId, ProcUpdate),
}

pub trait CustomProcCmd: Any + Debug {}

impl ProcCmd {
  pub fn custom<T: CustomProcCmd + Send>(custom: T) -> Self {
    Self::Custom(Box::new(custom))
  }
}

#[derive(Debug)]
pub enum ProcEvent {
  Exited(u32),
  Started,
  SetVt(Option<SharedVt>),
}

pub enum ProcUpdate {
  Started,
  Stopped(u32),
  Waiting(bool),
  Rendered,
  ScreenChanged(Option<SharedVt>),
}

impl Debug for ProcUpdate {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Started => write!(f, "Started"),
      Self::Stopped(code) => f.debug_tuple("Stopped").field(code).finish(),
      Self::Waiting(waiting) => {
        f.debug_tuple("Waiting").field(waiting).finish()
      }
      Self::Rendered => write!(f, "Rendered"),
      Self::ScreenChanged(arg0) => f
        .debug_tuple("ScreenChanged")
        .field(&arg0.is_some())
        .finish(),
    }
  }
}
