#![feature(iter_intersperse)]

mod app;
mod config;
mod encode_term;
mod event;
mod keymap;
mod proc;
mod state;
mod theme;
mod ui_keymap;
mod ui_procs;
mod ui_term;

use clap::Parser;
use flexi_logger::FileSpec;

use crate::app::App;

/// Run multiple processes in parallel and see output
#[derive(clap::Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// Config path
  #[clap(short, long, default_value_t = String::from("mprocs.json"))]
  config: String,
}

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
  let args = Args::parse();

  let app = App::from_config_file(args.config)?;
  app.run().await
}
