use std::ffi::OsString;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::process::process_spec::ProcessSpec;
use crate::term::key::Key;

#[derive(Clone)]
pub struct ProcConfig {
  pub name: String,
  pub cmd: CmdConfig,
  pub cwd: Option<OsString>,
  pub env: Option<IndexMap<String, Option<String>>>,
  pub autostart: bool,
  pub autorestart: bool,
  pub stop: StopSignal,
  pub deps: Vec<String>,
  pub mouse_scroll_speed: usize,
  pub scrollback_len: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CmdConfig {
  Cmd { cmd: Vec<String> },
  Shell { shell: String },
}

#[derive(Clone, Debug, Default)]
pub enum StopSignal {
  SIGINT,
  #[default]
  SIGTERM,
  SIGKILL,
  SendKeys(Vec<Key>),
  HardKill,
}

impl From<&ProcConfig> for ProcessSpec {
  fn from(cfg: &ProcConfig) -> Self {
    let mut cmd = match &cfg.cmd {
      CmdConfig::Cmd { cmd } => ProcessSpec::from_argv(cmd.clone()),
      CmdConfig::Shell { shell } => cmd_from_shell(shell),
    };

    if let Some(env) = &cfg.env {
      for (k, v) in env {
        if let Some(v) = v {
          cmd.env(k, v);
        } else {
          cmd.env_remove(k);
        }
      }
    }

    if let Some(cwd) = &cfg.cwd {
      cmd.cwd(cwd.to_string_lossy());
    } else if let Ok(cwd) = std::env::current_dir() {
      cmd.cwd(cwd.to_string_lossy());
    }

    cmd
  }
}

#[cfg(windows)]
fn cmd_from_shell(shell: &str) -> ProcessSpec {
  ProcessSpec::from_argv(vec![
    "pwsh.exe".into(),
    "-Command".into(),
    shell.into(),
  ])
}

#[cfg(not(windows))]
fn cmd_from_shell(shell: &str) -> ProcessSpec {
  ProcessSpec::from_argv(vec!["/bin/sh".into(), "-c".into(), shell.into()])
}
