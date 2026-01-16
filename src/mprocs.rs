use std::{
  io::Read,
  path::{Path, PathBuf},
};

use crate::app::{client_loop, create_app_proc, ClientId};
use crate::client::client_main;
use crate::config::{
  CmdConfig, Config, ConfigContext, ProcConfig, ServerConfig,
};
use crate::config_lua::load_lua_config;
use crate::ctl::run_ctl;
#[cfg(unix)]
use crate::error::ResultLogger;
use crate::host::{
  receiver::MsgReceiver, sender::MsgSender, socket::bind_server_socket,
};
use crate::just::load_just_procs;
use crate::kernel::{
  kernel::Kernel,
  kernel_message::KernelCommand,
  proc::{ProcInit, ProcStatus},
};
use crate::keymap::Keymap;
use crate::package_json::load_npm_procs;
use crate::proc::StopSignal;
use crate::settings::Settings;
use crate::yaml_val::Val;
use anyhow::{bail, Result};
use clap::{arg, command, ArgMatches};
use flexi_logger::{FileSpec, LoggerHandle};
use serde_yaml::Value;

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

pub async fn mprocs_main() -> anyhow::Result<()> {
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
    .arg(arg!(--"on-all-finished" [YAML] "Event to trigger when all processes are finished"))
    .arg(arg!(--"log-dir" [DIR] "Directory for process log files. Each process logs to <DIR>/<name>.log"))
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
      Config::make_default(&settings)?
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

    if let Some(on_all_finished) = matches.get_one::<String>("on-all-finished")
    {
      config.on_all_finished = Some(serde_yaml::from_str(on_all_finished)?);
    }

    if let Some(log_dir) = matches.get_one::<String>("log-dir") {
      config.log_dir = Some(PathBuf::from(log_dir));
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
          deps: Vec::new(),
          mouse_scroll_speed: settings.mouse_scroll_speed,
          scrollback_len: settings.scrollback_len,
          log_dir: config.log_dir.clone(),
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

    // Propagate log_dir to all procs (for yaml/npm/just loaded procs)
    if config.log_dir.is_some() {
      for proc in &mut config.procs {
        if proc.log_dir.is_none() {
          proc.log_dir = config.log_dir.clone();
        }
      }
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
    Some(("server", _args)) => {
      let logger = setup_logger(LogTarget::Stderr);

      #[cfg(unix)]
      crate::process::unix_processes_waiter::UnixProcessesWaiter::init()?;
      let mut kernel = Kernel::new();
      kernel.spawn_proc(|pc| {
        let app_proc_id = create_app_proc(config, keymap, &pc);
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();

        let app_sender = pc.get_proc_sender(app_proc_id);

        tokio::spawn(async move {
          let mut last_client_id = 0;

          let mut server_socket = match bind_server_socket().await {
            Ok(server_socket) => {
              log::info!("Server is listening.");
              server_socket
            }
            Err(err) => {
              log::error!("Failed to bind the server: {:?}", err);
              pc.send(KernelCommand::Quit);
              return;
            }
          };
          log::debug!("Waiting for clients...");
          loop {
            match server_socket.accept().await {
              Ok(socket) => {
                last_client_id += 1;
                let client_id = ClientId(last_client_id);
                let app_sender = app_sender.clone();
                tokio::spawn(async move {
                  client_loop(client_id, app_sender, socket).await;
                });
              }
              Err(err) => {
                log::info!("Server socket accept error: {}", err);
                break;
              }
            }
          }
        });

        ProcInit {
          sender,
          stop_on_quit: false,
          status: ProcStatus::Down,
          deps: Vec::new(),
        }
      });

      kernel.run().await;
      #[cfg(unix)]
      crate::process::unix_processes_waiter::UnixProcessesWaiter::uninit()?;

      drop(logger);
      Ok(())
    }
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

      #[cfg(unix)]
      crate::process::unix_processes_waiter::UnixProcessesWaiter::init()?;
      let mut kernel = Kernel::new();
      kernel.spawn_proc(|pc| {
        let app_proc_id = create_app_proc(config, keymap, &pc);
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();

        let app_sender = pc.get_proc_sender(app_proc_id);
        tokio::spawn(async move {
          client_loop(
            ClientId(1),
            app_sender,
            (srv_to_clt_sender, clt_to_srv_receiver),
          )
          .await
        });

        ProcInit {
          sender,
          stop_on_quit: false,
          status: ProcStatus::Down,
          deps: Vec::new(),
        }
      });
      tokio::spawn(async {
        kernel.run().await;
        #[cfg(unix)]
        crate::process::unix_processes_waiter::UnixProcessesWaiter::uninit()
          .log_ignore();
      });

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
