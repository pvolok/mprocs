use std::path::PathBuf;

use anyhow::Result;

use crate::cfg::{CfgCx, CfgObj};

#[derive(Clone, Default)]
pub struct LogConfig {
  /// `off|error|warn|info|debug|trace`, or an env_logger spec.
  pub level: Option<String>,
  pub file: Option<PathBuf>,
}

impl LogConfig {
  pub(crate) fn merge(&mut self, obj: &CfgObj<'_>, cx: &CfgCx) -> Result<()> {
    let log_obj = match obj.get("log") {
      Some(node) => node.as_obj()?,
      None => return Ok(()),
    };

    if let Some(level) = log_obj.optional::<String>("level", cx)? {
      self.level = Some(level);
    }
    if let Some(file) = log_obj.get("file") {
      self.file = Some(cx.resolve_path(file.as_str()?));
    }

    Ok(())
  }
}
