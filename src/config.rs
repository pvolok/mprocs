use std::{env::consts::OS, fs::File, io::BufReader, path::Path, rc::Rc};

use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};

use indexmap::IndexMap;
use serde_json::Value;

pub struct Config {
  pub procs: Vec<ProcConfig>,
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
    let config = Val::new(&config)?;
    let config = config.as_object()?;

    let procs = if let Some(procs) = config.get("procs") {
      let procs = procs
        .as_object()?
        .into_iter()
        .map(|(name, proc)| ProcConfig::from_val(name, proc))
        .collect::<Vec<_>>()
        .lift()?;
      procs
    } else {
      Vec::new()
    };

    let config = Config { procs };

    Ok(config)
  }
}

impl Default for Config {
  fn default() -> Self {
    Self { procs: Vec::new() }
  }
}

pub struct ProcConfig {
  pub name: String,
  pub cmd: CmdConfig,
  pub cwd: Option<String>,
  pub env: Option<IndexMap<String, Option<String>>>,
}

impl ProcConfig {
  fn from_val(name: String, val: Val) -> anyhow::Result<ProcConfig> {
    match val.0 {
      Value::Null => todo!(),
      Value::Bool(_) => todo!(),
      Value::Number(_) => todo!(),
      Value::String(shell) => Ok(ProcConfig {
        name,
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
          name,
          cmd: CmdConfig::Cmd { cmd },
          cwd: None,
          env: None,
        })
      }
      Value::Object(_) => {
        let map = val.as_object()?;

        let cmd = {
          let shell = map.get("shell");
          let cmd = map.get("cmd");

          match (shell, cmd) {
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
          }
        };

        let env = match map.get("env") {
          Some(env) => {
            let env = env.as_object()?;
            let env = env
              .into_iter()
              .map(|(k, v)| {
                let v = match v.0 {
                  Value::Null => Ok(None),
                  Value::String(v) => Ok(Some(v.to_owned())),
                  _ => Err(v.error_at("Expected string or null")),
                };
                (k, v)
              })
              .collect::<IndexMap<_, _>>()
              .lift()?;
            Some(env)
          }
          None => None,
        };

        Ok(ProcConfig {
          name,
          cmd,
          cwd: None,
          env,
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
      for (k, v) in env {
        if let Some(v) = v {
          cmd.env(k, v);
        } else {
          cmd.env_remove(k);
        }
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
  pub fn new(value: &'a Value) -> anyhow::Result<Self> {
    Self::create(value, Trace::empty())
  }

  pub fn create(value: &'a Value, trace: Trace) -> anyhow::Result<Self> {
    match value {
      Value::Object(map) => {
        if map.keys().next().map_or(false, |k| k.eq("$select")) {
          let (v, t) = Self::select(map, trace.clone())?;
          return Self::create(v, t);
        }
      }
      _ => (),
    }
    Ok(Val(value, trace))
  }

  fn select(
    map: &'a serde_json::Map<String, Value>,
    trace: Trace,
  ) -> anyhow::Result<(&'a Value, Trace)> {
    if map.get("$select").unwrap() == "os" {
      if let Some(v) = map.get(OS) {
        return Ok((v, trace.add(OS)));
      }

      if let Some(v) = map.get("$else") {
        return Ok((v, trace.add("$else")));
      }

      anyhow::bail!(
        "No matching condition found at {}. Use \"$else\" for default value.",
        trace.to_string(),
      )
    } else {
      anyhow::bail!("Expected \"os\" at {}", trace.add("$select").to_string())
    }
  }

  pub fn error_at<T: AsRef<str>>(&self, msg: T) -> anyhow::Error {
    anyhow::format_err!("{} at {}", msg.as_ref(), self.1.to_string())
  }

  pub fn as_str(&self) -> anyhow::Result<&str> {
    self.0.as_str().ok_or_else(|| {
      anyhow::format_err!("Expected string at {}", self.1.to_string())
    })
  }

  pub fn as_array(&self) -> anyhow::Result<Vec<Val>> {
    self
      .0
      .as_array()
      .ok_or_else(|| {
        anyhow::format_err!("Expected array at {}", self.1.to_string())
      })?
      .iter()
      .enumerate()
      .map(|(i, item)| Val::create(item, self.1.add(i)))
      .collect::<Vec<_>>()
      .lift()
  }

  pub fn as_object(&self) -> anyhow::Result<IndexMap<String, Val>> {
    self
      .0
      .as_object()
      .ok_or_else(|| {
        anyhow::format_err!("Expected object at {}", self.1.to_string())
      })?
      .iter()
      .map(|(k, item)| (k.to_owned(), Val::create(item, self.1.add(k))))
      .collect::<IndexMap<_, _>>()
      .lift()
  }
}
