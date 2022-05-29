use serde::{Deserialize, Serialize};

use crate::key::Key;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "c", rename_all = "kebab-case")]
pub enum AppEvent {
  Quit,
  ForceQuit,

  ToggleScope,

  NextProc,
  PrevProc,
  StartProc,
  TermProc,
  KillProc,
  RestartProc,
  ForceRestartProc,
  ShowAddProc,
  AddProc { cmd: String },

  ScrollDown,
  ScrollUp,

  SendKey { key: Key },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn serialize() {
    assert_eq!(
      serde_yaml::to_string(&AppEvent::ForceQuit).unwrap(),
      "---\nc: force-quit\n"
    );

    assert_eq!(
      serde_yaml::to_string(&AppEvent::SendKey {
        key: Key::parse("<c-a>").unwrap()
      })
      .unwrap(),
      "---\nc: send-key\nkey: \"<C-a>\"\n"
    );
  }
}
