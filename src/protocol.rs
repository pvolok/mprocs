use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::term::TermEvent;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SrvToClt {
  Print(String),
  Flush,
  Quit,
}

#[derive(
  Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq, Serialize,
)]
pub enum CursorStyle {
  #[default]
  Default = 0,
  BlinkingBlock = 1,
  SteadyBlock = 2,
  BlinkingUnderline = 3,
  SteadyUnderline = 4,
  BlinkingBar = 5,
  SteadyBar = 6,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum CltToSrv {
  Init { width: u16, height: u16 },
  Key(TermEvent),
}
