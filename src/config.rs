use std::path::Path;

use anyhow::Result;

use crate::cfg::{CfgCx, CfgDoc, CfgNode, FromCfg};

const CONFIG_FILE: &str = "dekit.yaml";

#[derive(Debug, Default)]
pub struct Config {
  pub procs: Vec<ProcConfig>,
}

#[derive(Debug)]
pub struct ProcConfig {
  pub name: String,
  pub cmd: Vec<String>,
  pub cwd: Option<String>,
}

impl Config {
  pub fn load(working_dir: &Path) -> Result<Self> {
    let path = working_dir.join(CONFIG_FILE);
    if !path.exists() {
      return Ok(Self::default());
    }
    let cx = CfgCx::new(working_dir.to_path_buf());
    let doc = CfgDoc::load(&path, &cx)?;
    doc.root().parse(&cx)
  }
}

impl FromCfg for Config {
  fn from_cfg(node: &CfgNode<'_>, cx: &CfgCx) -> Result<Self> {
    let obj = node.as_obj()?;
    let procs = obj.default("procs", Vec::new(), cx)?;
    Ok(Self { procs })
  }
}

impl FromCfg for ProcConfig {
  fn from_cfg(node: &CfgNode<'_>, cx: &CfgCx) -> Result<Self> {
    let obj = node.as_obj()?;
    Ok(Self {
      name: obj.required("name", cx)?,
      cmd: obj.required("cmd", cx)?,
      cwd: obj.optional("cwd", cx)?,
    })
  }
}
