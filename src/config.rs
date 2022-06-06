use std::{ffi::OsString, path::PathBuf};

use anyhow::Result;
use indexmap::IndexMap;
use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::yaml_val::{value_to_string, Val};

pub struct ConfigContext {
  pub path: PathBuf,
}

pub struct Config {
  pub procs: Vec<ProcConfig>,
  pub server: Option<ServerConfig>,
}

impl Config {
  pub fn from_value(value: &Value, ctx: &ConfigContext) -> Result<Config> {
    let config = Val::new(value)?;
    let config = config.as_object()?;

    let procs = if let Some(procs) = config.get(&Value::from("procs")) {
      let procs = procs
        .as_object()?
        .into_iter()
        .map(|(name, proc)| {
          Ok(ProcConfig::from_val(value_to_string(&name)?, proc, ctx)?)
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|x| x)
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

    let config = Config { procs, server };

    Ok(config)
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      procs: Vec::new(),
      server: None,
    }
  }
}

pub struct ProcConfig {
  pub name: String,
  pub cmd: CmdConfig,
  pub cwd: Option<OsString>,
  pub env: Option<IndexMap<String, Option<String>>>,
}

impl ProcConfig {
  fn from_val(
    name: String,
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
              if let Some(parent) = ctx.path.canonicalize()?.parent() {
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

        Ok(Some(ProcConfig {
          name,
          cmd,
          cwd,
          env,
        }))
      }
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

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum CmdConfig {
  Cmd { cmd: Vec<String> },
  Shell { shell: String },
}

impl From<&ProcConfig> for CommandBuilder {
  fn from(cfg: &ProcConfig) -> Self {
    let mut cmd = match &cfg.cmd {
      CmdConfig::Cmd { cmd } => {
        let (head, tail) = cmd.split_at(1);
        let mut cmd = CommandBuilder::new(&head[0]);
        cmd.args(tail);
        cmd
      }
      CmdConfig::Shell { shell } => {
        if cfg!(target_os = "windows") {
          let mut cmd = CommandBuilder::new("cmd");
          cmd.args(["/C", &shell]);
          cmd
        } else {
          let mut cmd = CommandBuilder::new("sh");
          cmd.arg("-c");
          cmd.arg(&shell);
          cmd
        }
      }
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
      cmd.cwd(cwd);
    } else if let Ok(cwd) = std::env::current_dir() {
      cmd.cwd(cwd);
    }

    cmd
  }
}
