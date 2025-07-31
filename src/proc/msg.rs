use std::{any::Any, fmt::Debug};

use compact_str::CompactString;

use crate::{
  kernel2::{kernel_message::SharedVt, proc::ProcId},
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

  Resize { x: u16, y: u16, w: u16, h: u16 },

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
  Render,
  Exited(u32),
  StdoutEOF,
  Started,
  SetVt(Option<SharedVt>),
  TermReply(CompactString),
}

pub enum ProcUpdate {
  Started,
  Stopped(u32),
  Rendered,
  ScreenChanged(Option<SharedVt>),
}

impl Debug for ProcUpdate {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Started => write!(f, "Started"),
      Self::Stopped(code) => f.debug_tuple("Stopped").field(code).finish(),
      Self::Rendered => write!(f, "Rendered"),
      Self::ScreenChanged(arg0) => f
        .debug_tuple("ScreenChanged")
        .field(&arg0.is_some())
        .finish(),
    }
  }
}
