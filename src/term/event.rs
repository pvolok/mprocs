use serde::{Deserialize, Serialize};

use super::key::Key;
use super::mouse::MouseEvent;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum TermEvent {
  FocusGained,
  FocusLost,
  Key(Key),
  Mouse(MouseEvent),
  Paste(String),
  Resize(u16, u16),
}
