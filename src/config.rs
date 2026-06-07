use std::{ffi::OsString, path::Path, path::PathBuf};

use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::cfg::{CfgCx, CfgDoc, CfgNode, CfgObj};
use crate::console::action::Action;
use crate::console::proc::StopSignal;
use crate::mprocs::{proc_log_config::LogConfig, settings::Settings};
use crate::process::process_spec::ProcessSpec;

#[derive(Clone)]
pub enum Hook {
  Action(Action),
}

impl Hook {
  pub fn as_action(&self) -> &Action {
    let Hook::Action(action) = self;
    action
  }
}

pub struct Config {
  pub procs: Vec<ProcConfig>,
  pub server: Option<ServerConfig>,
  pub hide_keymap_window: bool,
  pub mouse_scroll_speed: usize,
  pub scrollback_len: usize,
  pub proc_list_width: usize,
  pub proc_list_title: String,
  pub on_init: Option<Hook>,
  pub on_all_finished: Option<Hook>,
  pub proc_log: Option<LogConfig>,
}

impl Config {
  pub fn make_default(settings: &Settings) -> anyhow::Result<Self> {
    Ok(Self {
      procs: Vec::new(),
      server: None,
      hide_keymap_window: settings.hide_keymap_window,
      mouse_scroll_speed: settings.mouse_scroll_speed,
      scrollback_len: settings.scrollback_len,
      proc_list_width: settings.proc_list_width,
      proc_list_title: settings.proc_list_title.clone(),
      on_init: None,
      on_all_finished: settings.on_all_finished.clone().map(Hook::Action),
      proc_log: settings.proc_log.clone(),
    })
  }

  pub fn load_dir(working_dir: &Path, settings: &Settings) -> Result<Config> {
    let path = working_dir.join("dekit.yaml");
    if !path.exists() {
      return Config::make_default(settings);
    }

    let cx = CfgCx::new(working_dir.to_path_buf());
    let doc = CfgDoc::load(&path, &cx)?;
    let root = doc.root();
    let obj = root.as_obj()?;

    let procs = match obj.get("procs") {
      Some(node) => node
        .as_arr()?
        .iter()
        .map(|proc| proc_from_cfg(&proc, settings, &cx))
        .collect::<Result<Vec<_>>>()?,
      None => Vec::new(),
    };

    let server = match obj.get("server") {
      Some(node) => Some(ServerConfig::from_str(node.as_str()?)?),
      None => None,
    };

    let proc_list_title =
      obj.default("proc_list_title", settings.proc_list_title.clone(), &cx)?;

    let on_init = event_from_cfg(&obj, "on_init")?;
    let on_all_finished = event_from_cfg(&obj, "on_all_finished")?
      .or_else(|| settings.on_all_finished.clone().map(Hook::Action));

    let proc_log: Option<LogConfig> = obj.optional("proc_log", &cx)?;
    let proc_log = proc_log.or_else(|| settings.proc_log.clone());

    Ok(Config {
      procs,
      server,
      hide_keymap_window: settings.hide_keymap_window,
      mouse_scroll_speed: settings.mouse_scroll_speed,
      scrollback_len: settings.scrollback_len,
      proc_list_width: settings.proc_list_width,
      proc_list_title,
      on_init,
      on_all_finished,
      proc_log,
    })
  }
}

fn proc_from_cfg(
  node: &CfgNode<'_>,
  settings: &Settings,
  cx: &CfgCx,
) -> Result<ProcConfig> {
  let obj = node.as_obj()?;

  let name: String = obj.required("name", cx)?;

  let cmd = match (obj.get("shell"), obj.get("cmd")) {
    (Some(shell), None) => CmdConfig::Shell {
      shell: shell.as_str()?.to_owned(),
    },
    (None, Some(cmd)) => CmdConfig::Cmd {
      cmd: cmd
        .as_arr()?
        .iter()
        .map(|item| Ok(item.as_str()?.to_owned()))
        .collect::<Result<Vec<_>>>()?,
    },
    (None, None) => bail!(obj.error("process must define 'cmd' or 'shell'")),
    (Some(_), Some(_)) => {
      bail!(obj.error("process must define only one of 'cmd' or 'shell'"))
    }
  };

  let cwd = match obj.get("cwd") {
    Some(node) => Some(cx.resolve_path(node.as_str()?).into_os_string()),
    None => None,
  };

  let env: Option<IndexMap<String, Option<String>>> =
    obj.optional("env", cx)?;
  let add_path: Vec<PathBuf> = obj.default("add_path", Vec::new(), cx)?;
  let stop: StopSignal = obj.default("stop", StopSignal::default(), cx)?;
  let log: Option<LogConfig> = obj.optional("log", cx)?;

  Ok(ProcConfig {
    name,
    cmd,
    cwd,
    env,
    add_path,
    autostart: obj.default("autostart", true, cx)?,
    autorestart: obj.default("autorestart", false, cx)?,
    stop,
    deps: obj.default("deps", Vec::new(), cx)?,
    mouse_scroll_speed: settings.mouse_scroll_speed,
    scrollback_len: settings.scrollback_len,
    log,
  })
}

fn event_from_cfg(obj: &CfgObj<'_>, key: &str) -> Result<Option<Hook>> {
  match obj.get(key) {
    Some(node) => {
      let action: Action = serde_yaml::from_value(node.raw().clone())?;
      Ok(Some(Hook::Action(action)))
    }
    None => Ok(None),
  }
}

#[derive(Clone)]
pub struct ProcConfig {
  pub name: String,
  pub cmd: CmdConfig,
  pub cwd: Option<OsString>,
  pub env: Option<IndexMap<String, Option<String>>>,
  pub add_path: Vec<PathBuf>,
  pub autostart: bool,
  pub autorestart: bool,

  pub stop: StopSignal,

  pub deps: Vec<String>,

  pub mouse_scroll_speed: usize,
  pub scrollback_len: usize,
  pub log: Option<LogConfig>,
}

pub enum ServerConfig {
  Tcp(String),
}

impl ServerConfig {
  pub fn from_str(server_addr: &str) -> Result<Self> {
    Ok(Self::Tcp(server_addr.to_string()))
  }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CmdConfig {
  Cmd { cmd: Vec<String> },
  Shell { shell: String },
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

    if !cfg.add_path.is_empty() {
      let base = cfg
        .env
        .as_ref()
        .and_then(|env| env.get("PATH").cloned().flatten())
        .or_else(|| std::env::var("PATH").ok());
      let mut paths: Vec<PathBuf> = base
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
      paths.extend(cfg.add_path.iter().cloned());
      if let Ok(joined) = std::env::join_paths(&paths) {
        cmd.env("PATH", joined.to_string_lossy().into_owned());
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
pub fn cmd_from_shell(shell: &str) -> ProcessSpec {
  ProcessSpec::from_argv(vec![
    "pwsh.exe".into(),
    "-Command".into(),
    shell.into(),
  ])
}

#[cfg(not(windows))]
pub fn cmd_from_shell(shell: &str) -> ProcessSpec {
  ProcessSpec::from_argv(vec!["/bin/sh".into(), "-c".into(), shell.into()])
}

//
// Legacy mprocs config -> canonical dekit config
//

impl From<crate::mprocs::config::Config> for Config {
  fn from(legacy: crate::mprocs::config::Config) -> Self {
    Config {
      procs: legacy.procs.into_iter().map(ProcConfig::from).collect(),
      server: legacy.server.map(ServerConfig::from),
      hide_keymap_window: legacy.hide_keymap_window,
      mouse_scroll_speed: legacy.mouse_scroll_speed,
      scrollback_len: legacy.scrollback_len,
      proc_list_width: legacy.proc_list_width,
      proc_list_title: legacy.proc_list_title,
      on_init: legacy.on_init.map(Hook::Action),
      on_all_finished: legacy.on_all_finished.map(Hook::Action),
      proc_log: legacy.proc_log,
    }
  }
}

impl From<crate::mprocs::config::ProcConfig> for ProcConfig {
  fn from(legacy: crate::mprocs::config::ProcConfig) -> Self {
    ProcConfig {
      name: legacy.name,
      cmd: legacy.cmd.into(),
      cwd: legacy.cwd,
      env: legacy.env,
      add_path: legacy.add_path,
      autostart: legacy.autostart,
      autorestart: legacy.autorestart,
      stop: legacy.stop,
      deps: legacy.deps,
      mouse_scroll_speed: legacy.mouse_scroll_speed,
      scrollback_len: legacy.scrollback_len,
      log: legacy.log,
    }
  }
}

impl From<crate::mprocs::config::CmdConfig> for CmdConfig {
  fn from(legacy: crate::mprocs::config::CmdConfig) -> Self {
    match legacy {
      crate::mprocs::config::CmdConfig::Cmd { cmd } => CmdConfig::Cmd { cmd },
      crate::mprocs::config::CmdConfig::Shell { shell } => {
        CmdConfig::Shell { shell }
      }
    }
  }
}

impl From<crate::mprocs::config::ServerConfig> for ServerConfig {
  fn from(legacy: crate::mprocs::config::ServerConfig) -> Self {
    match legacy {
      crate::mprocs::config::ServerConfig::Tcp(addr) => ServerConfig::Tcp(addr),
    }
  }
}
