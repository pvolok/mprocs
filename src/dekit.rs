use anyhow::anyhow;
use clap::{Arg, Command};

use crate::lualib::init_std;

pub async fn dekit_main() -> anyhow::Result<()> {
  println!("* Welcome to dekit — playground for future features *\n");

  let cmd = clap::command!()
    .subcommands([
      Command::new("up"),
      Command::new("down"),
      Command::new("server").subcommands([
        Command::new("start"),
        Command::new("stop"),
        Command::new("status"),
      ]),
    ])
    .arg(
      Arg::new("files")
        .action(clap::ArgAction::Append)
        .trailing_var_arg(true),
    );
  let matches = cmd.get_matches();

  match matches.subcommand() {
    Some(("up", _sub_m)) => {
      println!("Up.");
    }
    Some(("down", _sub_m)) => {
      println!("Down.");
    }
    Some(("server", sub_m)) => {
      match sub_m.subcommand() {
        Some(("start", _sub_m)) => {
          println!("Start server.");
        }
        Some(("stop", _sub_m)) => {
          println!("Stop server.");
        }
        Some(("status", _sub_m)) => {
          println!("Server status.");
        }
        _ => {
          //
        }
      }
    }
    Some((arg, _sub_m)) => {
      println!("Unknown: {}", arg);
    }
    None => {
      println!("None.");
      let paths = matches
        .get_many::<String>("files")
        .map(|p| p.collect::<Vec<_>>())
        .unwrap_or_default();
      println!("paths = {:?}", paths);

      #[allow(clippy::collapsible_if)]
      if let Some(first) = paths.first() {
        if first.ends_with(".lua") {
          println!("Running the script.");

          let src = std::fs::read_to_string(first)?;

          let lua = mlua::Lua::new();
          let cancel = tokio_util::sync::CancellationToken::new();
          lua.set_app_data(cancel.clone());
          lua
            .globals()
            .set("std", init_std(&lua).map_err(|e| anyhow!("{}", e))?)
            .map_err(|e| anyhow!("{}", e))?;
          println!("After std init.");

          let chunk = lua.load(src.clone());
          let f: mlua::Function = chunk.eval().map_err(|e| anyhow!("{}", e))?;
          let r = f
            .call_async::<mlua::Value>(())
            .await
            .map_err(|e| anyhow!("{}", e))?;
          println!("-> {:?}", r);
          cancel.cancel();
        }
      }
    }
  }

  Ok(())
}
