use std::{fs::File, io::BufReader, path::Path};

use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};

use indexmap::IndexMap;

#[derive(Deserialize, Serialize)]
pub struct Config {
  pub procs: IndexMap<String, ProcConfig>,
}

impl Config {
  pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Config> {
    // Open the file in read-only mode with buffer.
    let file = match File::open(&path) {
      Ok(file) => file,
      Err(err) => match err.kind() {
        std::io::ErrorKind::NotFound => {
          return Err(anyhow::anyhow!(
            "Config file '{}' not found.",
            path.as_ref().display()
          ))
        }
        _kind => return Err(err.into()),
      },
    };
    let reader = BufReader::new(file);

    let config = serde_json::from_reader(reader)?;

    Ok(config)
  }
}

#[derive(Deserialize, Serialize)]
pub struct ProcConfig {
  #[serde(flatten)]
  pub cmd: CmdConfig,
  #[serde(default)]
  pub cwd: Option<String>,
  #[serde(default)]
  pub env: Option<Vec<String>>,
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
      for entry in env {
        let (k, v) = entry.split_once('=').unwrap_or((&entry, ""));
        cmd.env(k, v);
      }
    }

    let cwd = match &cfg.cwd {
      Some(cwd) => Some(cwd.clone()),
      None => std::env::current_dir()
        .ok()
        .map(|cd| cd.as_path().to_string_lossy().to_string()),
    };
    if let Some(cwd) = cwd {
      cmd.cwd(cwd);
    }

    cmd
  }
}
