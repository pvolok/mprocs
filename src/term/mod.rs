use serde::{Deserialize, Serialize};

use crate::{key::Key, mouse::MouseEvent};

mod input_parser;
mod internal;
pub mod line_symbols;
pub mod term_driver;
#[cfg(windows)]
mod windows;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum TermEvent {
  FocusGained,
  FocusLost,
  Key(Key),
  Mouse(MouseEvent),
  Paste(String),
  Resize(u16, u16),
}
