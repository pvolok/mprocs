mod app;
mod config;
mod ctl;
mod encode_term;
mod event;
mod keymap;
mod proc;
mod state;
mod theme;
mod ui_add_proc;
mod ui_keymap;
mod ui_procs;
mod ui_term;

use std::path::Path;

use clap::{arg, command};
use config::{CmdConfig, Config, ProcConfig, ServerConfig};
use ctl::run_ctl;
use flexi_logger::FileSpec;

use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
  let _logger = flexi_logger::Logger::try_with_str("info")
    .unwrap()
    .log_to_file(FileSpec::default().suppress_timestamp())
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
    .arg(arg!(-c --config [PATH] "Config path").default_value("mprocs.json"))
    .arg(arg!(-s --server [PATH] "Remote control server address. Example: 127.0.0.1:4050."))
    .arg(arg!(--ctl [JSON] "Send json encoded command to running mprocs"))
    .arg(arg!([COMMANDS]... "Commands to run (if omitted, commands from config will be run)"))
    .get_matches();

  let mut config = {
    let config_required = matches.occurrences_of("COMMANDS") == 0
      || matches.occurrences_of("config") > 0;
    let config_path = Path::new(matches.value_of("config").unwrap());

    if config_required || config_path.is_file() {
      Config::from_file(config_path)?
    } else {
      Config::default()
    }
  };

  if let Some(server_addr) = matches.value_of("server") {
    config.server = Some(ServerConfig::from_str(server_addr)?);
  }

  if matches.occurrences_of("ctl") > 0 {
    return run_ctl(matches.value_of("ctl").unwrap(), &config).await;
  }

  if let Some(cmds) = matches.values_of("COMMANDS") {
    let procs = cmds
      .into_iter()
      .map(|cmd| ProcConfig {
        name: cmd.to_owned(),
        cmd: CmdConfig::Shell {
          shell: cmd.to_owned(),
        },
        env: None,
        cwd: None,
      })
      .collect::<Vec<_>>();

    config.procs = procs;
  }

  let app = App::from_config_file(config)?;
  app.run().await
}
