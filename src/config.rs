use std::{ffi::OsString, path::PathBuf, str::FromStr};

use anyhow::{bail, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::{
  event::AppEvent,
  proc::StopSignal,
  process::process_spec::ProcessSpec,
  settings::Settings,
  yaml_val::{value_to_string, Val},
};

pub struct ConfigContext {
  pub path: PathBuf,
}

fn resolve_config_path(path: &str, ctx: &ConfigContext) -> Result<PathBuf> {
  let mut buf = PathBuf::new();
  if let Some(rest) = path.strip_prefix("<CONFIG_DIR>") {
    if let Some(parent) = dunce::canonicalize(&ctx.path)?.parent() {
      buf.push(parent);
    }
    buf.push(rest.trim_start_matches(['/', '\\']));
  } else {
    buf.push(path);
  }

  Ok(buf)
}

/// Configuration for a process group in the sidebar
#[derive(Clone, Debug)]
pub struct GroupConfig {
  pub name: String,
  pub collapsed: bool,
  pub proc_names: Vec<String>,
}

pub struct Config {
  pub procs: Vec<ProcConfig>,
  pub groups: Vec<GroupConfig>,
  pub server: Option<ServerConfig>,
  pub hide_keymap_window: bool,
  pub mouse_scroll_speed: usize,
  pub scrollback_len: usize,
  pub proc_list_width: usize,
  pub proc_list_title: String,
  pub on_all_finished: Option<AppEvent>,
  pub log_dir: Option<PathBuf>,
}

impl Config {
  pub fn from_value(
    value: &Value,
    ctx: &ConfigContext,
    settings: &Settings,
  ) -> Result<Config> {
    let config = Val::new(value)?;
    let config = config.as_object()?;

    let procs = if let Some(procs) = config.get(&Value::from("procs")) {
      let procs = procs
        .as_object()?
        .into_iter()
        .map(|(name, proc)| {
          ProcConfig::from_val(
            value_to_string(&name)?,
            settings.mouse_scroll_speed,
            settings.scrollback_len,
            proc,
            ctx,
          )
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
      procs
    } else {
      Vec::new()
    };

    let server = if let Some(addr) = config.get(&Value::from("server")) {
      Some(ServerConfig::from_str(addr.as_str()?)?)
    } else {
      None
    };

    let proc_list_title =
      if let Some(title) = config.get(&Value::from("proc_list_title")) {
        title.as_str()?.to_string()
      } else {
        settings.proc_list_title.clone()
      };

    let on_all_finished =
      if let Some(val) = config.get(&Value::from("on_all_finished")) {
        Some(serde_yaml::from_value(val.raw().clone())?)
      } else {
        settings.on_all_finished.clone()
      };

    let log_dir = match config.get(&Value::from("log_dir")) {
      Some(val) => match val.raw() {
        Value::Null => None,
        Value::String(log_dir) => Some(resolve_config_path(log_dir, ctx)?),
        _ => return Err(val.error_at("Expected string or null")),
      },
      None => match &settings.log_dir {
        Some(log_dir) => Some(resolve_config_path(log_dir, ctx)?),
        None => None,
      },
    };

    // Parse groups section
    let proc_names: std::collections::HashSet<_> =
      procs.iter().map(|p| p.name.as_str()).collect();
    let groups = if let Some(groups_val) = config.get(&Value::from("groups")) {
      let groups_map = groups_val.as_object()?;
      let mut groups = Vec::new();
      for (name, group_val) in groups_map {
        let group_name = value_to_string(&name)?;
        let group_obj = group_val.as_object()?;

        let collapsed = group_obj
          .get(&Value::from("collapsed"))
          .map_or(Ok(false), |v| v.as_bool())?;

        let proc_names_in_group =
          if let Some(procs_val) = group_obj.get(&Value::from("procs")) {
            procs_val
              .as_array()?
              .iter()
              .filter_map(|p| {
                let name = p.as_str().ok()?;
                if proc_names.contains(name) {
                  Some(name.to_string())
                } else {
                  log::warn!(
                    "Group '{}' references unknown process '{}'",
                    group_name,
                    name
                  );
                  None
                }
              })
              .collect()
          } else {
            Vec::new()
          };

        groups.push(GroupConfig {
          name: group_name,
          collapsed,
          proc_names: proc_names_in_group,
        });
      }
      groups
    } else {
      Vec::new()
    };

    let config = Config {
      procs,
      groups,
      server,
      hide_keymap_window: settings.hide_keymap_window,
      mouse_scroll_speed: settings.mouse_scroll_speed,
      scrollback_len: settings.scrollback_len,
      proc_list_width: settings.proc_list_width,
      proc_list_title,
      on_all_finished,
      log_dir,
    };

    Ok(config)
  }

  pub fn make_default(settings: &Settings) -> anyhow::Result<Self> {
    Ok(Self {
      procs: Vec::new(),
      groups: Vec::new(),
      server: None,
      hide_keymap_window: settings.hide_keymap_window,
      mouse_scroll_speed: settings.mouse_scroll_speed,
      scrollback_len: settings.scrollback_len,
      proc_list_width: settings.proc_list_width,
      proc_list_title: settings.proc_list_title.clone(),
      on_all_finished: settings.on_all_finished.clone(),
      log_dir: settings.log_dir.as_ref().map(PathBuf::from),
    })
  }
}

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
  pub log_dir: Option<PathBuf>,
}

impl ProcConfig {
  fn from_val(
    name: String,
    mouse_scroll_speed: usize,
    scrollback_len: usize,
    val: Val,
    ctx: &ConfigContext,
  ) -> Result<Option<ProcConfig>> {
    match val.raw() {
      Value::Null => Ok(None),
      Value::Bool(_) => todo!(),
      Value::Number(_) => todo!(),
      Value::String(shell) => Ok(Some(ProcConfig {
        name,
        cmd: CmdConfig::Shell {
          shell: shell.to_owned(),
        },
        cwd: None,
        env: None,
        autostart: true,
        autorestart: false,
        stop: StopSignal::default(),
        deps: Vec::new(),

        mouse_scroll_speed,
        scrollback_len,
        log_dir: None,
      })),
      Value::Sequence(_) => {
        let cmd = val.as_array()?;
        let cmd = cmd
          .into_iter()
          .map(|item| item.as_str().map(|s| s.to_owned()))
          .collect::<Result<Vec<_>>>()?;

        Ok(Some(ProcConfig {
          name,
          cmd: CmdConfig::Cmd { cmd },
          cwd: None,
          env: None,
          autostart: true,
          autorestart: false,
          stop: StopSignal::default(),
          deps: Vec::new(),
          mouse_scroll_speed,
          scrollback_len,
          log_dir: None,
        }))
      }
      Value::Mapping(_) => {
        let map = val.as_object()?;

        let cmd = {
          let shell = map.get(&Value::from("shell"));
          let cmd = map.get(&Value::from("cmd"));

          match (shell, cmd) {
            (None, Some(cmd)) => CmdConfig::Cmd {
              cmd: cmd
                .as_array()?
                .into_iter()
                .map(|v| v.as_str().map(|s| s.to_owned()))
                .collect::<Result<Vec<_>>>()?,
            },
            (Some(shell), None) => CmdConfig::Shell {
              shell: shell.as_str()?.to_owned(),
            },
            (None, None) => todo!(),
            (Some(_), Some(_)) => todo!(),
          }
        };

        let cwd = match map.get(&Value::from("cwd")) {
          Some(cwd) => {
            let cwd = cwd.as_str()?;
            let mut buf = OsString::new();
            if let Some(rest) = cwd.strip_prefix("<CONFIG_DIR>") {
              if let Some(parent) = dunce::canonicalize(&ctx.path)?.parent() {
                buf.push(parent);
              }
              buf.push(rest);
            } else {
              buf.push(cwd);
            }
            Some(buf)
          }
          None => None,
        };

        let log_dir = match map.get(&Value::from("log_dir")) {
          Some(val) => match val.raw() {
            Value::Null => None,
            Value::String(_) => Some(resolve_config_path(val.as_str()?, ctx)?),
            _ => return Err(val.error_at("Expected string or null")),
          },
          None => None,
        };

        let env = match map.get(&Value::from("env")) {
          Some(env) => {
            let env = env.as_object()?;
            let env = env
              .into_iter()
              .map(|(k, v)| {
                let v = match v.raw() {
                  Value::Null => Ok(None),
                  Value::String(v) => Ok(Some(v.to_owned())),
                  _ => Err(v.error_at("Expected string or null")),
                };
                Ok((value_to_string(&k)?, v?))
              })
              .collect::<Result<IndexMap<_, _>>>()?;
            Some(env)
          }
          None => None,
        };
        let env = match map.get(&Value::from("add_path")) {
          Some(add_path) => {
            let extra_paths = match add_path.raw() {
              Value::String(path) => vec![path.as_str()],
              Value::Sequence(paths) => paths
                .iter()
                .filter_map(|path| path.as_str())
                .collect::<Vec<_>>(),
              _ => {
                bail!(add_path.error_at("Expected string or array"));
              }
            };
            let extra_paths = extra_paths
              .into_iter()
              .map(|p| PathBuf::from_str(p).map_err(anyhow::Error::from))
              .collect::<Result<Vec<_>>>()?;
            let mut paths = std::env::var_os("PATH").map_or_else(
              || Vec::new(),
              |path_var| {
                std::env::split_paths(&path_var)
                  .map(|p| p.into_os_string())
                  .collect::<Vec<_>>()
              },
            );
            for p in extra_paths {
              paths.push(p.into_os_string());
            }
            let path_var =
              std::env::join_paths(paths)?.to_string_lossy().to_string();
            let env = if let Some(mut env) = env {
              env.insert("PATH".to_string(), Some(path_var));
              env
            } else {
              let mut env = IndexMap::with_capacity(1);
              env.insert("PATH".to_string(), Some(path_var));
              env
            };
            Some(env)
          }
          None => env,
        };

        let autostart = map
          .get(&Value::from("autostart"))
          .map_or(Ok(true), |v| v.as_bool())?;

        let autorestart = map
          .get(&Value::from("autorestart"))
          .map_or(Ok(false), |v| v.as_bool())?;

        let stop_signal = if let Some(val) = map.get(&Value::from("stop")) {
          StopSignal::from_val(val)?
        } else {
          StopSignal::default()
        };

        let deps = if let Some(deps) = map.get(&Value::from("deps")) {
          deps
            .as_array()?
            .iter()
            .map(|d| d.as_str().map(|s| s.to_string()))
            .collect::<Result<Vec<_>>>()?
        } else {
          Vec::new()
        };

        Ok(Some(ProcConfig {
          name,
          cmd,
          cwd,
          env,
          autostart,
          autorestart,
          stop: stop_signal,
          deps,
          mouse_scroll_speed,
          scrollback_len,
          log_dir,
        }))
      }
      Value::Tagged(_) => anyhow::bail!("Yaml tags are not supported"),
    }
  }
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

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  fn make_test_settings() -> Settings {
    Settings::default()
  }

  fn make_test_ctx() -> ConfigContext {
    ConfigContext {
      path: PathBuf::from("/tmp/mprocs.yaml"),
    }
  }

  #[test]
  fn test_groups_parsing_basic() {
    let yaml = r#"
procs:
  server: "node server.js"
  client: "npm run dev"
groups:
  backend:
    procs:
      - server
"#;
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let settings = make_test_settings();
    let ctx = make_test_ctx();
    let config = Config::from_value(&value, &ctx, &settings).unwrap();

    assert_eq!(config.groups.len(), 1);
    assert_eq!(config.groups[0].name, "backend");
    assert_eq!(config.groups[0].proc_names, vec!["server".to_string()]);
    assert!(!config.groups[0].collapsed);
  }

  #[test]
  fn test_groups_parsing_collapsed() {
    let yaml = r#"
procs:
  server: "node server.js"
groups:
  backend:
    collapsed: true
    procs:
      - server
"#;
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let settings = make_test_settings();
    let ctx = make_test_ctx();
    let config = Config::from_value(&value, &ctx, &settings).unwrap();

    assert_eq!(config.groups.len(), 1);
    assert!(config.groups[0].collapsed);
  }

  #[test]
  fn test_groups_parsing_empty_groups() {
    let yaml = r#"
procs:
  server: "node server.js"
"#;
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let settings = make_test_settings();
    let ctx = make_test_ctx();
    let config = Config::from_value(&value, &ctx, &settings).unwrap();

    assert!(config.groups.is_empty());
  }

  #[test]
  fn test_groups_parsing_unknown_proc_filtered() {
    let yaml = r#"
procs:
  server: "node server.js"
groups:
  backend:
    procs:
      - server
      - unknown_proc
"#;
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let settings = make_test_settings();
    let ctx = make_test_ctx();
    let config = Config::from_value(&value, &ctx, &settings).unwrap();

    assert_eq!(config.groups.len(), 1);
    // unknown_proc should be filtered out
    assert_eq!(config.groups[0].proc_names, vec!["server".to_string()]);
  }

  #[test]
  fn test_groups_parsing_multiple_groups() {
    let yaml = r#"
procs:
  server: "node server.js"
  client: "npm run dev"
  tests: "npm test"
groups:
  backend:
    procs:
      - server
  frontend:
    collapsed: true
    procs:
      - client
"#;
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let settings = make_test_settings();
    let ctx = make_test_ctx();
    let config = Config::from_value(&value, &ctx, &settings).unwrap();

    assert_eq!(config.groups.len(), 2);
    // Note: YAML map ordering may vary, so we check by finding groups by name
    let backend = config.groups.iter().find(|g| g.name == "backend").unwrap();
    let frontend = config.groups.iter().find(|g| g.name == "frontend").unwrap();

    assert!(!backend.collapsed);
    assert_eq!(backend.proc_names, vec!["server".to_string()]);

    assert!(frontend.collapsed);
    assert_eq!(frontend.proc_names, vec!["client".to_string()]);
  }

  #[test]
  fn test_groups_parsing_empty_procs_list() {
    let yaml = r#"
procs:
  server: "node server.js"
groups:
  empty_group:
    collapsed: false
"#;
    let value: Value = serde_yaml::from_str(yaml).unwrap();
    let settings = make_test_settings();
    let ctx = make_test_ctx();
    let config = Config::from_value(&value, &ctx, &settings).unwrap();

    assert_eq!(config.groups.len(), 1);
    assert_eq!(config.groups[0].name, "empty_group");
    assert!(config.groups[0].proc_names.is_empty());
  }
}
