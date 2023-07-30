mod app;
mod client;
mod clipboard;
mod config;
mod config_lua;
mod ctl;
mod dk_screen;
mod encode_term;
mod error;
mod event;
mod key;
mod keymap;
mod package_json;
mod proc;
mod protocol;
mod settings;
mod state;
mod theme;
mod ui_add_proc;
mod ui_confirm_quit;
mod ui_keymap;
mod ui_procs;
mod ui_remove_proc;
mod ui_term;
mod ui_zoom_tip;
mod yaml_val;

use std::{io::Read, path::Path};

use anyhow::{bail, Result};
use app::server_main;
use clap::{arg, command, ArgMatches};
use client::client_main;
use config::{CmdConfig, Config, ConfigContext, ProcConfig, ServerConfig};
use config_lua::load_lua_config;
use ctl::run_ctl;
use flexi_logger::FileSpec;
use keymap::Keymap;
use package_json::load_npm_procs;
use proc::StopSignal;
use protocol::{CltToSrv, SrvToClt};
use serde_yaml::Value;
use settings::Settings;
use yaml_val::Val;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
  let logger_str = if cfg!(debug_assertions) {
    "debug"
  } else {
    "warn"
  };
  let _logger = flexi_logger::Logger::try_with_str(logger_str)
    .unwrap()
    .log_to_file(FileSpec::default().suppress_timestamp())
    .append()
    .use_utc()
    .start()
    .unwrap();

  match run_app().await {
    Ok(()) => Ok(()),
    Err(err) => {
      eprintln!("Error: {}", err);
      Ok(())
    }
  }
}

async fn run_app() -> anyhow::Result<()> {
  let matches = command!()
    .arg(arg!(-c --config [PATH] "Config path [default: mprocs.yaml]"))
    .arg(arg!(-s --server [PATH] "Remote control server address. Example: 127.0.0.1:4050."))
    .arg(arg!(--ctl [YAML] "Send yaml/json encoded command to running mprocs"))
    .arg(arg!(--names [NAMES] "Names for processes provided by cli arguments. Separated by comma."))
    .arg(arg!(--npm "Run scripts from package.json. Scripts are not started by default."))
    .arg(arg!([COMMANDS]... "Commands to run (if omitted, commands from config will be run)"))
    .get_matches();

  let config_value = load_config_value(&matches)
    .map_err(|e| anyhow::Error::msg(format!("[{}] {}", "config", e)))?;

  let mut settings = Settings::default();

  // merge ~/.config/mprocs/mprocs.yaml
  settings.merge_from_xdg().map_err(|e| {
    anyhow::Error::msg(format!("[{}] {}", "global settings", e))
  })?;
  // merge ./mprocs.yaml
  if let Some((value, _)) = &config_value {
    settings
      .merge_value(Val::new(value)?)
      .map_err(|e| anyhow::Error::msg(format!("[{}] {}", "local config", e)))?;
  }

  let mut keymap = Keymap::new();
  settings.add_to_keymap(&mut keymap)?;

  let config = {
    let mut config = if let Some((v, ctx)) = config_value {
      Config::from_value(&v, &ctx, &settings)?
    } else {
      Config::make_default(&settings)
    };

    if let Some(server_addr) = matches.get_one::<String>("server") {
      config.server = Some(ServerConfig::from_str(server_addr)?);
    }

    if let Some(ctl_arg) = matches.get_one::<String>("ctl") {
      return run_ctl(ctl_arg, &config).await;
    }

    if let Some(cmds) = matches.get_many::<String>("COMMANDS") {
      let names = matches
        .get_one::<String>("names")
        .map_or_else(|| Vec::new(), |arg| arg.split(",").collect::<Vec<_>>());
      let procs = cmds
        .into_iter()
        .enumerate()
        .map(|(i, cmd)| ProcConfig {
          name: names
            .get(i)
            .map_or_else(|| cmd.to_string(), |s| s.to_string()),
          cmd: CmdConfig::Shell {
            shell: cmd.to_string(),
          },
          env: None,
          cwd: None,
          autostart: true,
          stop: StopSignal::default(),
        })
        .collect::<Vec<_>>();

      config.procs = procs;
    } else if matches.contains_id("npm") {
      let procs = load_npm_procs()?;
      config.procs = procs;
    }

    config
  };

  run_client_and_server(config, keymap).await
}

async fn run_client_and_server(config: Config, keymap: Keymap) -> Result<()> {
  let (clt_tx, srv_rx) = tokio::sync::mpsc::channel::<CltToSrv>(64);
  let (srv_tx, clt_rx) = tokio::sync::mpsc::unbounded_channel::<SrvToClt>();

  let client = tokio::spawn(async { client_main(clt_tx, clt_rx).await });
  let server =
    tokio::spawn(async { server_main(config, keymap, srv_tx, srv_rx).await });

  let r1 = server
    .await
    .unwrap_or_else(|err| Err(anyhow::Error::from(err)));
  let r2 = client
    .await
    .unwrap_or_else(|err| Err(anyhow::Error::from(err)));

  r1.and(r2)
    .map_err(|err| anyhow::Error::msg(err.to_string()))
}

fn load_config_value(
  matches: &ArgMatches,
) -> Result<Option<(Value, ConfigContext)>> {
  if let Some(path) = matches.get_one::<String>("config") {
    return Ok(Some((
      read_value(path)?,
      ConfigContext { path: path.into() },
    )));
  }

  {
    let path = "mprocs.lua";
    if Path::new(path).is_file() {
      return Ok(Some((
        read_value(path)?,
        ConfigContext { path: path.into() },
      )));
    }
  }

  {
    let path = "mprocs.yaml";
    if Path::new(path).is_file() {
      return Ok(Some((
        read_value(path)?,
        ConfigContext { path: path.into() },
      )));
    }
  }

  {
    let path = "mprocs.json";
    if Path::new(path).is_file() {
      return Ok(Some((
        read_value(path)?,
        ConfigContext { path: path.into() },
      )));
    }
  }

  Ok(None)
}

fn read_value(path: &str) -> Result<Value> {
  // Open the file in read-only mode with buffer.
  let file = match std::fs::File::open(&path) {
    Ok(file) => file,
    Err(err) => match err.kind() {
      std::io::ErrorKind::NotFound => {
        bail!("Config file '{}' not found.", path);
      }
      _kind => return Err(err.into()),
    },
  };
  let mut reader = std::io::BufReader::new(file);
  let ext = std::path::Path::new(path)
    .extension()
    .map_or_else(|| "".to_string(), |ext| ext.to_string_lossy().to_string());
  let value: Value = match ext.as_str() {
    "yaml" | "yml" | "json" => serde_yaml::from_reader(reader)?,
    "lua" => {
      let mut buf = String::new();
      reader.read_to_string(&mut buf)?;
      load_lua_config(path, &buf)?
    }
    _ => bail!("Supported config extensions: lua, yaml, yml, json."),
  };
  Ok(value)
}
