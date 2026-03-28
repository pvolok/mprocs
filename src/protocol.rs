use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::term::TermEvent;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SrvToClt {
  Print(String),
  Flush,
  Quit,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum CltToSrv {
  Init { width: u16, height: u16 },
  Key(TermEvent),
}
