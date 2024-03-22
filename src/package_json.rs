use std::{fs::File, io::BufReader};

use anyhow::Result;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::{
  config::{CmdConfig, ProcConfig},
  proc::StopSignal,
  settings::Settings,
};

#[derive(Deserialize)]
struct Package {
  scripts: IndexMap<String, String>,
}

pub fn load_npm_procs(settings: &Settings) -> Result<Vec<ProcConfig>> {
  let file = File::open("package.json")?;
  let reader = BufReader::new(file);
  let package: Package = serde_yaml::from_reader(reader)?;

  let mut paths = if let Ok(path_var) = std::env::var("PATH") {
    let paths = std::env::split_paths(&path_var)
      .map(|p| p.to_string_lossy().to_string())
      .collect::<Vec<_>>();
    paths
  } else {
    Vec::with_capacity(1)
  };
  paths.push("./node_modules/.bin".to_string());
  let mut env = IndexMap::with_capacity(1);
  env.insert(
    "PATH".to_string(),
    Some(std::env::join_paths(paths)?.into_string().map_err(|_| {
      anyhow::Error::msg(
        "Failed to set PATH variable while loading package.json.",
      )
    })?),
  );

  let procs = package.scripts.into_iter().map(|(name, cmd)| ProcConfig {
    name,
    cmd: CmdConfig::Shell { shell: cmd },
    cwd: None,
    env: Some(env.clone()),
    autostart: false,
    autorestart: false,

    stop: StopSignal::default(),
    mouse_scroll_speed: settings.mouse_scroll_speed,
  });
  Ok(procs.collect())
}
