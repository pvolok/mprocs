use anyhow::Result;
use indexmap::IndexMap;
use serde_json::Value;

use crate::{
  config::{CmdConfig, ProcConfig},
  proc::StopSignal,
  settings::Settings,
};

#[derive(serde::Deserialize)]
struct Justfile {
  recipes: IndexMap<String, Value>,
}

// Use `just` command to get recipes from a justfile
pub fn load_just_procs(settings: &Settings) -> Result<Vec<ProcConfig>> {
  let output = std::process::Command::new("just")
    .arg("--dump")
    .arg("--dump-format=json")
    // Specify the justfile to avoid loading the default justfile
    .arg("--justfile=justfile")
    .output()
    .map_err(|e| anyhow::Error::msg(format!("Failed to run just: {}", e)))?;

  if !output.status.success() {
    return Err(anyhow::Error::msg(format!(
      "Failed to run just: {}",
      String::from_utf8_lossy(&output.stderr)
    )));
  }

  let justfile: Justfile = serde_json::from_slice(&output.stdout)?;
  let procs = justfile
    .recipes
    .into_iter()
    .map(|(name, _recipes)| ProcConfig {
      name: name.clone(),
      cmd: CmdConfig::Shell {
        shell: format!("just {}", name.clone()),
      },
      cwd: None,
      env: None,
      autostart: false,
      autorestart: false,

      stop: StopSignal::default(),
      mouse_scroll_speed: settings.mouse_scroll_speed,
      scrollback_len: settings.scrollback_len,
    });
  Ok(procs.collect())
}
