use std::{
  path::PathBuf,
  time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use serde_yaml::Value;

use crate::yaml_val::Val;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LogMode {
  #[default]
  Append,
  Truncate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogConfig {
  pub enabled: Option<bool>,
  pub dir: Option<PathBuf>,
  pub file: Option<PathBuf>,
  pub mode: Option<LogMode>,
}

impl LogConfig {
  pub fn disabled() -> Self {
    Self {
      enabled: Some(false),
      dir: None,
      file: None,
      mode: None,
    }
  }

  pub fn with_dir(dir: PathBuf) -> Self {
    Self {
      enabled: Some(true),
      dir: Some(dir),
      file: None,
      mode: None,
    }
  }

  pub fn enabled(&self) -> bool {
    self.enabled.unwrap_or(true)
  }

  pub fn mode(&self) -> LogMode {
    self.mode.unwrap_or(LogMode::Append)
  }

  pub fn merged(&self, child: &LogConfig) -> LogConfig {
    LogConfig {
      enabled: child.enabled.or(self.enabled),
      dir: child.dir.clone().or_else(|| self.dir.clone()),
      file: child.file.clone().or_else(|| self.file.clone()),
      mode: child.mode.or(self.mode),
    }
  }

  pub fn file_path(
    &self,
    proc_name: &str,
    proc_id: usize,
    pid: u32,
  ) -> Option<PathBuf> {
    if !self.enabled() {
      return None;
    }

    let dir = self.dir.as_ref().map(|dir| {
      PathBuf::from(expand_template(
        &dir.to_string_lossy(),
        proc_name,
        proc_id,
        pid,
      ))
    });
    let file = self.file.as_ref().map(|file| {
      PathBuf::from(expand_template(
        &file.to_string_lossy(),
        proc_name,
        proc_id,
        pid,
      ))
    });

    match (dir, file) {
      (None, None) => None,
      (Some(dir), None) => Some(dir.join(default_log_filename(proc_name))),
      (None, Some(file)) => Some(file),
      (Some(dir), Some(file)) if file.is_relative() => Some(dir.join(file)),
      (Some(_dir), Some(file)) => Some(file),
    }
  }
}

pub fn default_log_filename(name: &str) -> String {
  let mut out = String::new();
  for ch in name.chars() {
    let is_safe = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.');
    if is_safe {
      out.push(ch);
    } else {
      out.push('_');
    }
  }

  let trimmed = out.trim_matches(|c| c == '.' || c == ' ').to_string();
  if trimmed.is_empty() {
    "process.log".to_string()
  } else {
    format!("{}.log", trimmed)
  }
}

pub fn parse_log_config<F>(
  val: &Val<'_>,
  mut resolve_path: F,
) -> Result<Option<LogConfig>>
where
  F: FnMut(&str) -> Result<PathBuf>,
{
  match val.raw() {
    Value::Null => Ok(None),
    Value::Bool(false) => Ok(Some(LogConfig::disabled())),
    Value::Bool(true) => Ok(Some(LogConfig {
      enabled: Some(true),
      dir: None,
      file: None,
      mode: None,
    })),
    Value::Number(_) => {
      Err(val.error_at("Expected bool, string, object, or null"))
    }
    Value::String(dir) => Ok(Some(LogConfig::with_dir(resolve_path(dir)?))),
    Value::Sequence(_) => {
      Err(val.error_at("Expected bool, string, object, or null"))
    }
    Value::Mapping(_) => {
      let map = val.as_object()?;

      let enabled = map
        .get(&Value::from("enabled"))
        .map_or(Ok(true), |v| v.as_bool())?;

      let dir = match map.get(&Value::from("dir")) {
        Some(dir) => match dir.raw() {
          Value::Null => None,
          Value::String(path) => Some(resolve_path(path)?),
          _ => return Err(dir.error_at("Expected string or null")),
        },
        None => None,
      };

      let file = match map.get(&Value::from("file")) {
        Some(file) => match file.raw() {
          Value::Null => None,
          Value::String(path) => Some(resolve_path(path)?),
          _ => return Err(file.error_at("Expected string or null")),
        },
        None => None,
      };

      let mode = match (
        map.get(&Value::from("mode")),
        map.get(&Value::from("append")),
      ) {
        (Some(_), Some(append)) => {
          return Err(
            append.error_at("Use either `mode` or `append`, not both"),
          );
        }
        (Some(mode), None) => match mode.as_str()? {
          "append" => LogMode::Append,
          "truncate" => LogMode::Truncate,
          _ => return Err(mode.error_at("Expected `append` or `truncate`")),
        },
        (None, Some(append)) => {
          if append.as_bool()? {
            LogMode::Append
          } else {
            LogMode::Truncate
          }
        }
        (None, None) => LogMode::Truncate,
      };

      Ok(Some(LogConfig {
        enabled: Some(enabled),
        dir,
        file,
        mode: Some(mode),
      }))
    }
    Value::Tagged(_) => anyhow::bail!("Yaml tags are not supported"),
  }
}

fn expand_template(
  template: &str,
  proc_name: &str,
  proc_id: usize,
  pid: u32,
) -> String {
  let ts = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| duration.as_secs())
    .unwrap_or(0);

  template
    .replace(
      "{name}",
      &default_log_filename(proc_name).trim_end_matches(".log"),
    )
    .replace("{id}", &proc_id.to_string())
    .replace("{pid}", &pid.to_string())
    .replace("{ts}", &ts.to_string())
}
