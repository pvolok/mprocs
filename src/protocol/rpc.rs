use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum DkRequest {
  Spawn {
    path: String,
    cmd: Vec<String>,
    cwd: Option<String>,
  },
  Ls {
    glob: Option<String>,
  },
  /// Start the autostart target.
  Up,
  /// Pin matching tasks to init and start them.
  Start {
    pattern: String,
  },
  /// Unpin matching tasks and stop their running instances; each comes
  /// back if something still wants it.
  Stop {
    pattern: String,
  },
  /// Unpin matching tasks; each stops only if nothing else wants it.
  Down {
    pattern: String,
  },
  /// Like `Stop` but with an immediate hard kill.
  Kill {
    pattern: String,
  },
  /// Keep matching tasks down until started again.
  KeepDown {
    pattern: String,
  },
  Restart {
    pattern: String,
  },
  /// Explain why a task is (not) running.
  Why {
    path: String,
  },
  Screen {
    path: String,
  },
  Shutdown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum DkResponse {
  Ok,
  TaskList(Vec<DkTaskInfo>),
  Screen(Option<String>),
  Why(DkWhy),
  Error(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DkTaskInfo {
  pub path: String,
  pub state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DkWhy {
  pub path: String,
  pub state: String,
  pub wanted: bool,
  pub supported: bool,
  pub kept_down: bool,
  pub pinned: bool,
  pub required_by: Vec<String>,
  pub deps: Vec<DkWhyDep>,
  pub attempts: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DkWhyDep {
  pub path: String,
  pub state: String,
  pub wanted: bool,
  pub satisfied: bool,
}
