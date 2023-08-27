use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::key::Key;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "c", rename_all = "kebab-case")]
pub enum AppEvent {
  Batch { cmds: Vec<AppEvent> },

  QuitOrAsk,
  Quit,
  ForceQuit,

  ToggleFocus,
  FocusProcs,
  FocusTerm,
  Zoom,

  NextProc,
  PrevProc,
  SelectProc { index: usize },
  StartProc,
  TermProc,
  KillProc,
  RestartProc,
  RenameProc { name: String },
  ForceRestartProc,
  ShowAddProc,
  ShowRenameProc,
  AddProc { cmd: String },
  ShowRemoveProc,
  RemoveProc { id: usize },

  ScrollDownLines { n: usize },
  ScrollUpLines { n: usize },
  ScrollDown,
  ScrollUp,

  CopyModeEnter,
  CopyModeLeave,
  CopyModeMove { dir: CopyMove },
  CopyModeEnd,
  CopyModeCopy,

  SendKey { key: Key },
}

impl AppEvent {
  pub fn desc(&self) -> String {
    match self {
      AppEvent::Batch { cmds: _ } => "Send multiple events".to_string(),
      AppEvent::QuitOrAsk => "Quit".to_string(),
      AppEvent::Quit => "Quit (without dialog)".to_string(),
      AppEvent::ForceQuit => "Force quit".to_string(),
      AppEvent::ToggleFocus => "Toggle focus".to_string(),
      AppEvent::FocusProcs => "Focus proccess list".to_string(),
      AppEvent::FocusTerm => "Focus terminal".to_string(),
      AppEvent::Zoom => "Zoom into terminal".to_string(),
      AppEvent::NextProc => "Next".to_string(),
      AppEvent::PrevProc => "Prev".to_string(),
      AppEvent::SelectProc { index } => format!("Select process #{}", index),
      AppEvent::StartProc => "Start".to_string(),
      AppEvent::TermProc => "Stop".to_string(),
      AppEvent::KillProc => "Kill".to_string(),
      AppEvent::RestartProc => "Restart".to_string(),
      AppEvent::RenameProc { name } => format!("Rename to \"{}\"", name),
      AppEvent::ForceRestartProc => "Force restart".to_string(),
      AppEvent::ShowAddProc => "New process dialog".to_string(),
      AppEvent::ShowRenameProc => "Rename process dialog".to_string(),
      AppEvent::AddProc { cmd } => format!("New process `{}`", cmd),
      AppEvent::ShowRemoveProc => "Remove process dialog".to_string(),
      AppEvent::RemoveProc { id } => format!("Remove process by id {}", id),
      AppEvent::ScrollDownLines { n } => {
        format!("Scroll down {} {}", n, lines_str(*n))
      }
      AppEvent::ScrollUpLines { n } => {
        format!("Scroll up {} {}", n, lines_str(*n))
      }
      AppEvent::ScrollDown => "Scroll down".to_string(),
      AppEvent::ScrollUp => "Scroll up".to_string(),
      AppEvent::CopyModeEnter => "Enter copy mode".to_string(),
      AppEvent::CopyModeLeave => "Leave copy mode".to_string(),
      AppEvent::CopyModeMove { dir } => {
        format!("Move selection cursor {}", dir)
      }
      AppEvent::CopyModeEnd => "Select end position".to_string(),
      AppEvent::CopyModeCopy => "Copy selected text".to_string(),
      AppEvent::SendKey { key } => format!("Send {} key", key.to_string()),
    }
  }
}

fn lines_str(n: usize) -> &'static str {
  if n == 1 {
    "line"
  } else {
    "lines"
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum CopyMove {
  Up,
  Right,
  Left,
  Down,
}

impl Display for CopyMove {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let str = match self {
      CopyMove::Up => "up",
      CopyMove::Right => "right",
      CopyMove::Left => "left",
      CopyMove::Down => "down",
    };
    f.write_str(str)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn serialize() {
    assert_eq!(
      serde_yaml::to_string(&AppEvent::ForceQuit).unwrap(),
      "c: force-quit\n"
    );

    assert_eq!(
      serde_yaml::to_string(&AppEvent::SendKey {
        key: Key::parse("<c-a>").unwrap()
      })
      .unwrap(),
      "c: send-key\nkey: <C-a>\n"
    );
  }
}
