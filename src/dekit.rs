use std::path::Path;

use anyhow::anyhow;
use clap::{Arg, Command};
use rquickjs::CatchResultExt;

use crate::{
  app::{client_loop, create_app_task, ClientId},
  client::client_main,
  config::Config,
  daemon::socket::{bind_server_socket, connect_client_socket},
  js::js_vm::JsVm,
  kernel::{
    kernel::Kernel,
    kernel_message::KernelCommand,
    task::{NoopTask, TaskInit, TaskStatus},
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
  kernel.spawn_task(|pc| {
    let app_task_id = create_app_task(config, keymap, &pc);

    let app_sender = pc.get_task_sender(app_task_id);

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

    TaskInit {
      task: Box::new(NoopTask),
      stop_on_quit: false,
      status: TaskStatus::Down,
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

          let vm = JsVm::new().await?;
          let root =
            vm.eval_file(Path::new("dekit.js"), src.as_bytes()).await?;

          let r: anyhow::Result<()> =
            rquickjs::async_with!(vm.context => |ctx| {
              run_module_main(&ctx, &root).await
            })
            .await;
          r?;
        }
      }
    }
  }

  Ok(())
}

async fn run_module_main(
  ctx: &rquickjs::Ctx<'_>,
  root: &rquickjs::Persistent<rquickjs::Object<'static>>,
) -> anyhow::Result<()> {
  let m = map_js_error(
    ctx,
    root.clone().restore(ctx),
    "Failed to restore module namespace",
  )?;
  let main = map_js_error(
    ctx,
    m.get::<_, rquickjs::Value>("main"),
    "Failed to read exported `main`",
  )?;

  let val = match main.type_of() {
    rquickjs::Type::Constructor => map_js_error(
      ctx,
      main
        .into_constructor()
        .expect("Type checked as constructor")
        .call::<_, rquickjs::Value>(()),
      "Error while calling exported constructor `main`",
    )?,
    rquickjs::Type::Function => map_js_error(
      ctx,
      main
        .into_function()
        .expect("Type checked as function")
        .call(()),
      "Error while calling exported function `main`",
    )?,
    t => anyhow::bail!("Exported `main` is not a function ({}).", t.as_str()),
  };

  let val = if let Some(promise) = val.clone().into_promise() {
    map_js_error(
      ctx,
      promise.into_future::<rquickjs::Value<'_>>().await,
      "Unhandled rejection in exported `main`",
    )?
  } else {
    val
  };

  println!("-> {:?}", val);
  Ok(())
}

fn map_js_error<T>(
  ctx: &rquickjs::Ctx<'_>,
  result: rquickjs::Result<T>,
  scope: &str,
) -> anyhow::Result<T> {
  result.catch(ctx).map_err(|err| anyhow!("{scope}:\n{err}"))
}
