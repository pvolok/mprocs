mod app;
mod client;
mod clipboard;
mod config;
mod config_lua;
mod ctl;
mod encode_term;
mod error;
mod event;
mod host;
mod just;
mod kernel;
mod key;
mod keymap;
mod modal;
mod mouse;
mod package_json;
mod proc;
mod protocol;
mod settings;
mod state;
mod theme;
mod ui_keymap;
mod ui_procs;
mod ui_term;
mod ui_zoom_tip;
mod widgets;
mod yaml_val;

use std::{io::Read, path::Path};

use anyhow::{bail, Result};
use app::{start_kernel_process, start_kernel_thread};
use clap::{arg, command, ArgMatches, Command};
use client::client_main;
use config::{CmdConfig, Config, ConfigContext, ProcConfig, ServerConfig};
use config_lua::load_lua_config;
use ctl::run_ctl;
use flexi_logger::{FileSpec, LoggerHandle};
use host::{receiver::MsgReceiver, sender::MsgSender};
use just::load_just_procs;
use keymap::Keymap;
use package_json::load_npm_procs;
use proc::StopSignal;
use serde_yaml::Value;
use settings::Settings;
use yaml_val::Val;

enum LogTarget {
  File,
  Stderr,
}

fn setup_logger(target: LogTarget) -> LoggerHandle {
  let logger_str = if cfg!(debug_assertions) {
    "debug"
  } else {
    "warn"
  };
  let logger = flexi_logger::Logger::try_with_str(logger_str).unwrap();
  let logger = match target {
    LogTarget::File => logger
      .log_to_file(FileSpec::default().suppress_timestamp())
      .append(),
    LogTarget::Stderr => logger.log_to_stderr(),
  };

  std::panic::set_hook(Box::new(|info| {
    let stacktrace = std::backtrace::Backtrace::capture();
    log::error!("Got panic. @info:{}\n@stackTrace:{}", info, stacktrace);
  }));

  logger.use_utc().start().unwrap()
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
  match run_app().await {
    Ok(()) => Ok(()),
    Err(err) => {
      eprintln!("Error: {:?}", err);
      Ok(())
    }
  }
}

async fn run_app() -> anyhow::Result<()> {
  let matches = command!()
    .arg(arg!(-c --config [PATH] "Config path [default: mprocs.yaml]"))
    .arg(arg!(-s --server [PATH] "Remote control server address. Example: 127.0.0.1:4050."))
    .arg(arg!(--ctl [YAML] "Send yaml/json encoded command to running mprocs"))
    .arg(arg!(--"proc-list-title" [TITLE] "Title for the processes pane"))
    .arg(arg!(--names [NAMES] "Names for processes provided by cli arguments. Separated by comma."))
    .arg(arg!(--npm "Run scripts from package.json. Scripts are not started by default."))
    .arg(arg!(--just "Run recipes from justfile. Recipes are not started by default. Requires just to be installed."))
    .arg(arg!([COMMANDS]... "Commands to run (if omitted, commands from config will be run)"))
    // .subcommand(Command::new("server"))
    // .subcommand(Command::new("attach"))
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

    if let Some(title) = matches.get_one::<String>("proc-list-title") {
      config.proc_list_title = title.to_string();
    }

    if let Some(cmds) = matches.get_many::<String>("COMMANDS") {
      let names = matches
        .get_one::<String>("names")
        .map_or(Vec::new(), |arg| arg.split(',').collect::<Vec<_>>());
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
          autorestart: false,
          stop: StopSignal::default(),
          mouse_scroll_speed: settings.mouse_scroll_speed,
          scrollback_len: settings.scrollback_len,
        })
        .collect::<Vec<_>>();

      config.procs = procs;
    } else if matches.get_flag("npm") {
      let procs = load_npm_procs(&settings)?;
      config.procs = procs;
    } else if matches.get_flag("just") {
      let procs = load_just_procs(&settings)?;
      config.procs = procs;
    }

    config
  };

  match matches.subcommand() {
    // Some(("attach", _args)) => {
    //   let logger = setup_logger(LogTarget::File);
    //   let ret = client_main(false).await;
    //   drop(logger);
    //   ret
    // }
    // Some(("server", _args)) => {
    //   let logger = setup_logger(LogTarget::Stderr);
    //   let ret = start_kernel_process(config, keymap).await;
    //   drop(logger);
    //   ret
    // }
    Some((cmd, _args)) => {
      bail!("Unexpected command: {}", cmd);
    }
    None => {
      let logger = setup_logger(LogTarget::File);

      let (srv_to_clt_sender, srv_to_clt_receiver) = {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let sender = MsgSender::new(sender);
        let receiver = MsgReceiver::new(receiver);
        (sender, receiver)
      };
      let (clt_to_srv_sender, clt_to_srv_receiver) = {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let sender = MsgSender::new(sender);
        let receiver = MsgReceiver::new(receiver);
        (sender, receiver)
      };

      start_kernel_thread(
        config,
        keymap,
        (srv_to_clt_sender, clt_to_srv_receiver),
      )
      .await?;

      let ret = client_main(clt_to_srv_sender, srv_to_clt_receiver).await;
      drop(logger);
      ret
    }
  }
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
  let file = match std::fs::File::open(path) {
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
  let mut value: Value = match ext.as_str() {
    "yaml" | "yml" | "json" => serde_yaml::from_reader(reader)?,
    "lua" => {
      let mut buf = String::new();
      reader.read_to_string(&mut buf)?;
      load_lua_config(path, &buf)?
    }
    _ => bail!("Supported config extensions: lua, yaml, yml, json."),
  };
  value.apply_merge().unwrap();
  Ok(value)
}
