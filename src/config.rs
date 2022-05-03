use std::{fs::File, io::BufReader, path::Path, rc::Rc};

use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};

use indexmap::IndexMap;
use serde_json::Value;

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

    let config: Value = serde_json::from_reader(reader)?;
    let config = Val::new(&config);
    let config = config.as_object()?;

    let procs = if let Some(procs) = config.get("procs") {
      let procs = procs
        .as_object()?
        .into_iter()
        .map(|(name, proc)| (name, ProcConfig::from_val(proc)))
        .collect::<IndexMap<_, _>>()
        .lift()?;
      procs
    } else {
      IndexMap::new()
    };

    let config = Config { procs };

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

impl ProcConfig {
  fn from_val(val: Val) -> anyhow::Result<ProcConfig> {
    match val.0 {
      Value::Null => todo!(),
      Value::Bool(_) => todo!(),
      Value::Number(_) => todo!(),
      Value::String(shell) => Ok(ProcConfig {
        cmd: CmdConfig::Shell {
          shell: shell.to_owned(),
        },
        cwd: None,
        env: None,
      }),
      Value::Array(_) => {
        let cmd = val.as_array()?;
        let cmd = cmd
          .into_iter()
          .map(|item| item.as_str().map(|s| s.to_owned()))
          .collect::<Vec<_>>();
        let cmd = cmd.lift()?;

        Ok(ProcConfig {
          cmd: CmdConfig::Cmd { cmd },
          cwd: None,
          env: None,
        })
      }
      Value::Object(_) => {
        let map = val.as_object()?;
        let shell = map.get("shell");
        let cmd = map.get("cmd");
        let cmd = match (shell, cmd) {
          (None, Some(cmd)) => CmdConfig::Cmd {
            cmd: cmd
              .as_array()?
              .into_iter()
              .map(|v| v.as_str().map(|s| s.to_owned()))
              .collect::<Vec<_>>()
              .lift()?,
          },
          (Some(shell), None) => CmdConfig::Shell {
            shell: shell.as_str()?.to_owned(),
          },
          (None, None) => todo!(),
          (Some(_), Some(_)) => todo!(),
        };

        Ok(ProcConfig {
          cmd,
          cwd: None,
          env: None,
        })
      }
    }
  }
}

trait ResultsVec<T> {
  fn lift(self) -> anyhow::Result<Vec<T>>;
}

impl<T> ResultsVec<T> for Vec<anyhow::Result<T>> {
  fn lift(self) -> anyhow::Result<Vec<T>> {
    let mut res = Vec::with_capacity(self.len());
    for item in self {
      res.push(item?);
    }
    Ok(res)
  }
}

trait ResultsMap<T> {
  fn lift(self) -> anyhow::Result<IndexMap<String, T>>;
}

impl<T> ResultsMap<T> for IndexMap<String, anyhow::Result<T>> {
  fn lift(self) -> anyhow::Result<IndexMap<String, T>> {
    let mut res = IndexMap::with_capacity(self.len());
    for (k, item) in self {
      res.insert(k, item?);
    }
    Ok(res)
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

#[derive(Clone)]
struct Trace(Option<Rc<Box<(String, Trace)>>>);

impl Trace {
  pub fn empty() -> Self {
    Trace(None)
  }

  pub fn add<T: ToString>(&self, seg: T) -> Self {
    Trace(Some(Rc::new(Box::new((seg.to_string(), self.clone())))))
  }

  pub fn to_string(&self) -> String {
    let mut str = String::new();
    fn add(buf: &mut String, trace: &Trace) {
      match &trace.0 {
        Some(part) => {
          add(buf, &part.1);
          buf.push('.');
          buf.push_str(&part.0);
        }
        None => buf.push_str("<config>"),
      }
    }
    add(&mut str, self);

    str
  }
}

struct Val<'a>(&'a Value, Trace);

impl<'a> Val<'a> {
  pub fn new(value: &'a Value) -> Self {
    Val(value, Trace::empty())
  }

  pub fn as_str(&self) -> anyhow::Result<&str> {
    self.0.as_str().ok_or_else(|| {
      anyhow::format_err!("Expected string at {}", self.1.to_string())
    })
  }

  pub fn as_array(&self) -> anyhow::Result<Vec<Val>> {
    Ok(
      self
        .0
        .as_array()
        .ok_or_else(|| {
          anyhow::format_err!("Expected array at {}", self.1.to_string())
        })?
        .iter()
        .enumerate()
        .map(|(i, item)| Val(item, self.1.add(i)))
        .collect(),
    )
  }

  pub fn as_object(&self) -> anyhow::Result<IndexMap<String, Val>> {
    Ok(
      self
        .0
        .as_object()
        .ok_or_else(|| {
          anyhow::format_err!("Expected object at {}", self.1.to_string())
        })?
        .iter()
        .map(|(k, item)| (k.to_owned(), Val(item, self.1.add(k))))
        .collect(),
    )
  }
}
