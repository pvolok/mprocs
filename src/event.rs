use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{
  app::ClientId, kernel::proc::ProcId, key::Key, proc::msg::CustomProcCmd,
};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "c", rename_all = "kebab-case")]
pub enum AppEvent {
  Batch { cmds: Vec<AppEvent> },

  QuitOrAsk,
  Quit,
  ForceQuit,
  Detach { client_id: ClientId },

  ToggleFocus,
  FocusProcs,
  FocusTerm,
  Zoom,

  ShowCommandsMenu,
  NextProc,
  PrevProc,
  SelectProc { index: usize },
  StartProc,
  TermProc,
  KillProc,
  RestartProc,
  RestartAll,
  RenameProc { name: String },
  ForceRestartProc,
  ForceRestartAll,
  ShowAddProc,
  ShowRenameProc,
  AddProc { cmd: String, name: Option<String> },
  DuplicateProc,
  ShowRemoveProc,
  RemoveProc { id: ProcId },

  CloseCurrentModal,

  ScrollDownLines { n: usize },
  ScrollUpLines { n: usize },
  ScrollDown,
  ScrollUp,

  CopyModeEnter,
  CopyModeLeave,
  CopyModeMove { dir: CopyMove },
  CopyModeEnd,
  CopyModeCopy,
  ToggleKeymapWindow,

  SendKey { key: Key },

  // Group operations
  ToggleGroup { name: String },
  CollapseGroup { name: String },
  ExpandGroup { name: String },
  CollapseAllGroups,
  ExpandAllGroups,
  ToggleSelectedGroup,
}

impl CustomProcCmd for AppEvent {}

impl AppEvent {
  pub fn desc(&self) -> String {
    match self {
      AppEvent::Batch { cmds: _ } => "Send multiple events".to_string(),
      AppEvent::QuitOrAsk => "Quit".to_string(),
      AppEvent::Quit => "Quit".to_string(),
      AppEvent::ForceQuit => "Force quit".to_string(),
      AppEvent::Detach { client_id } => {
        format!("Detach client #{:?}", client_id)
      }
      AppEvent::ToggleFocus => "Toggle focus".to_string(),
      AppEvent::FocusProcs => "Focus process list".to_string(),
      AppEvent::FocusTerm => "Focus terminal".to_string(),
      AppEvent::Zoom => "Zoom into terminal".to_string(),
      AppEvent::ShowCommandsMenu => "Show commands menu".to_string(),
      AppEvent::NextProc => "Next".to_string(),
      AppEvent::PrevProc => "Prev".to_string(),
      AppEvent::SelectProc { index } => format!("Select process #{}", index),
      AppEvent::StartProc => "Start".to_string(),
      AppEvent::TermProc => "Stop".to_string(),
      AppEvent::KillProc => "Kill".to_string(),
      AppEvent::RestartProc => "Restart".to_string(),
      AppEvent::RestartAll => "Restart all".to_string(),
      AppEvent::RenameProc { name } => format!("Rename to \"{}\"", name),
      AppEvent::ForceRestartProc => "Force restart".to_string(),
      AppEvent::ForceRestartAll => "Force restart all".to_string(),
      AppEvent::ShowAddProc => "New process dialog".to_string(),
      AppEvent::ShowRenameProc => "Rename process dialog".to_string(),
      AppEvent::AddProc { cmd, name: _ } => format!("New process `{}`", cmd),
      AppEvent::DuplicateProc => "Duplicate current process".to_string(),
      AppEvent::ShowRemoveProc => "Remove process dialog".to_string(),
      AppEvent::RemoveProc { id } => format!("Remove process by id {}", id.0),
      AppEvent::CloseCurrentModal => "Close current modal".to_string(),
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
      AppEvent::ToggleKeymapWindow => "Toggle help".to_string(),
      AppEvent::SendKey { key } => format!("Send {} key", key.to_string()),
      AppEvent::ToggleGroup { name } => format!("Toggle group: {}", name),
      AppEvent::CollapseGroup { name } => format!("Collapse group: {}", name),
      AppEvent::ExpandGroup { name } => format!("Expand group: {}", name),
      AppEvent::CollapseAllGroups => "Collapse all groups".to_string(),
      AppEvent::ExpandAllGroups => "Expand all groups".to_string(),
      AppEvent::ToggleSelectedGroup => "Toggle selected group".to_string(),
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

  #[test]
  fn serialize_group_events() {
    // Test ToggleGroup serialization
    assert_eq!(
      serde_yaml::to_string(&AppEvent::ToggleGroup {
        name: "backend".to_string()
      })
      .unwrap(),
      "c: toggle-group\nname: backend\n"
    );

    // Test CollapseGroup serialization
    assert_eq!(
      serde_yaml::to_string(&AppEvent::CollapseGroup {
        name: "frontend".to_string()
      })
      .unwrap(),
      "c: collapse-group\nname: frontend\n"
    );

    // Test ExpandGroup serialization
    assert_eq!(
      serde_yaml::to_string(&AppEvent::ExpandGroup {
        name: "test".to_string()
      })
      .unwrap(),
      "c: expand-group\nname: test\n"
    );

    // Test CollapseAllGroups serialization
    assert_eq!(
      serde_yaml::to_string(&AppEvent::CollapseAllGroups).unwrap(),
      "c: collapse-all-groups\n"
    );

    // Test ExpandAllGroups serialization
    assert_eq!(
      serde_yaml::to_string(&AppEvent::ExpandAllGroups).unwrap(),
      "c: expand-all-groups\n"
    );

    // Test deserialization
    let event: AppEvent =
      serde_yaml::from_str("c: toggle-group\nname: backend\n").unwrap();
    assert_eq!(
      event,
      AppEvent::ToggleGroup {
        name: "backend".to_string()
      }
    );

    let event: AppEvent =
      serde_yaml::from_str("c: collapse-all-groups\n").unwrap();
    assert_eq!(event, AppEvent::CollapseAllGroups);

    // Test ToggleSelectedGroup serialization
    assert_eq!(
      serde_yaml::to_string(&AppEvent::ToggleSelectedGroup).unwrap(),
      "c: toggle-selected-group\n"
    );

    // Test ToggleSelectedGroup deserialization
    let event: AppEvent =
      serde_yaml::from_str("c: toggle-selected-group\n").unwrap();
    assert_eq!(event, AppEvent::ToggleSelectedGroup);
  }
}
