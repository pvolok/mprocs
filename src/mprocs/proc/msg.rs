use std::fmt::Debug;

use crate::term::key::Key;

#[derive(Debug)]
pub enum ProcMsg {
  SendKey(Key),
}

#[derive(Debug)]
pub enum ProcEvent {
  Exited(u32),
  Started,
}
