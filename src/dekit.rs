use std::path::Path;

use anyhow::anyhow;
use clap::{Arg, Command};

use crate::{
  app::{client_loop, create_app_proc, ClientId},
  client::client_main,
  config::Config,
  host::socket::{bind_server_socket, connect_client_socket},
  js::js_vm::JsVm,
  kernel::{
    kernel::Kernel,
    kernel_message::KernelCommand,
    proc::{ProcInit, ProcStatus},
  },
  keymap::Keymap,
  lualib::init_std,
  protocol::{CltToSrv, SrvToClt},
  settings::Settings,
};

async fn run_server() -> anyhow::Result<()> {
  let settings = Settings::default();
  let mut keymap = Keymap::new();
  settings.add_to_keymap(&mut keymap)?;
  let config = Config::make_default(&settings)?;

  let _logger = {
    let logger_str = if cfg!(debug_assertions) {
      "debug"
    } else {
      "warn"
    };
    let logger = flexi_logger::Logger::try_with_str(logger_str)
      .unwrap()
      .log_to_file(flexi_logger::FileSpec::default().suppress_timestamp())
      .append()
      .duplicate_to_stdout(flexi_logger::Duplicate::All);

    std::panic::set_hook(Box::new(|info| {
      let stacktrace = std::backtrace::Backtrace::capture();
      log::error!("Got panic. @info:{}\n@stackTrace:{}", info, stacktrace);
    }));

    logger.use_utc().start().unwrap()
  };

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

  Ok(())
}

pub async fn dekit_main() -> anyhow::Result<()> {
  println!("* Welcome to dekit — playground for future features *\n");

  let cmd = clap::command!()
    .subcommands([
      Command::new("attach"),
      Command::new("up"),
      Command::new("down"),
      Command::new("server").subcommands([
        Command::new("run"),
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
    Some(("attach", _sub_m)) => {
      let (sender, receiver) =
        connect_client_socket::<CltToSrv, SrvToClt>(false).await?;
      client_main(sender, receiver).await?;
    }
    Some(("up", _sub_m)) => {
      println!("Up.");
    }
    Some(("down", _sub_m)) => {
      println!("Down.");
    }
    Some(("server", sub_m)) => match sub_m.subcommand() {
      Some(("run", _sub_m)) => {
        run_server().await?;
      }
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
        println!("Expected more arguments after `dk server`");
      }
    },
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
        // .lua
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
        // .js
        else if first.ends_with(".js") {
          println!("Running the script.");

          let src = std::fs::read_to_string(first)?;

          let vm = JsVm::new()?;
          let root = vm.eval_file(Path::new("dekit.js"), src.as_bytes())?;

          let r: anyhow::Result<()> = vm.context.with(|ctx| {
            let m = root.restore(&ctx)?;
            let r = m.get::<_, rquickjs::Value>("main")?;
            let r = match r.type_of() {
              rquickjs::Type::Constructor => {
                r.into_constructor().unwrap().call::<_, rquickjs::Value>(())
              }
              rquickjs::Type::Function => r.into_function().unwrap().call(()),
              t => {
                println!("Exported `main` is not a function ({}).", t.as_str());
                Ok(rquickjs::Value::new_undefined(ctx.clone()))
              }
            };
            println!("-> {:?}", r);
            if let Err(rquickjs::Error::Exception) = r {
              println!("Exc: {:?}", ctx.catch());
            }
            Ok(())
          });
          r?;
        }
      }
    }
  }

  Ok(())
}
