use std::path::{Path, PathBuf};

use anyhow::anyhow;
use clap::{Arg, Command};
use rquickjs::CatchResultExt;

use crate::{
  app::{client_loop, create_app_task, ClientId},
  client::client_main,
  config::Config,
  daemon::{
    lockfile,
    socket::{bind_server_socket, connect_client_socket},
  },
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

async fn run_server(working_dir: PathBuf) -> anyhow::Result<()> {
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

  // Create lock file and acquire exclusive flock.
  let lock_guard = lockfile::create_lock_file(&working_dir)?;
  log::info!("Lock file created for directory: {}", working_dir.display());

  #[cfg(unix)]
  crate::process::unix_processes_waiter::UnixProcessesWaiter::init()?;
  let mut kernel = Kernel::new();

  let socket_path = lock_guard.socket_path().to_path_buf();
  kernel.spawn_task(move |pc| {
    let app_task_id = create_app_task(config, keymap, &pc);

    let app_sender = pc.get_task_sender(app_task_id);

    tokio::spawn(async move {
      let mut last_client_id = 0;

      let mut server_socket = match bind_server_socket(&socket_path).await {
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

  // lock_guard is dropped here, removing lock + socket files.
  drop(lock_guard);

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
        Command::new("run").arg(
          Arg::new("dir")
            .long("dir")
            .required(true)
            .help("Working directory this daemon manages"),
        ),
        Command::new("start")
          .about("Start the daemon for the current directory"),
        Command::new("stop").about("Stop the daemon for the current directory"),
        Command::new("status")
          .about("Show daemon status for the current directory"),
        Command::new("list").about("List all daemons on this machine"),
        Command::new("clean").about("Remove stale lock files"),
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
      let working_dir = std::env::current_dir()?;
      let (sender, receiver) =
        connect_client_socket::<CltToSrv, SrvToClt>(&working_dir, false)
          .await?;
      client_main(sender, receiver).await?;
    }
    Some(("up", _sub_m)) => {
      println!("Up.");
    }
    Some(("down", _sub_m)) => {
      println!("Down.");
    }
    Some(("server", sub_m)) => match sub_m.subcommand() {
      Some(("run", run_m)) => {
        let dir = run_m.get_one::<String>("dir").unwrap();
        run_server(PathBuf::from(dir)).await?;
      }
      Some(("start", _sub_m)) => {
        let working_dir = std::env::current_dir()?;
        // Check if already running.
        if let Some(info) = lockfile::get_daemon_status(&working_dir)? {
          if info.is_running {
            println!("Daemon already running (pid={}).", info.contents.pid);
            return Ok(());
          }
          // Stale -- clean up and start fresh.
          lockfile::cleanup_stale(&working_dir)?;
        }
        crate::daemon::daemon::spawn_server_daemon(&working_dir)?;
        println!("Daemon started for {}.", working_dir.display());
      }
      Some(("stop", _sub_m)) => {
        let working_dir = std::env::current_dir()?;
        lockfile::stop_daemon(&working_dir)?;
        println!("Daemon stopped.");
      }
      Some(("status", _sub_m)) => {
        let working_dir = std::env::current_dir()?;
        match lockfile::get_daemon_status(&working_dir)? {
          Some(info) => {
            let status = if info.is_running { "running" } else { "stale" };
            println!(
              "[{}] pid={} socket={} version={}",
              status,
              info.contents.pid,
              info.contents.socket,
              info.contents.version,
            );
          }
          None => {
            println!("No daemon for this directory.");
          }
        }
      }
      Some(("list", _sub_m)) => {
        let daemons = lockfile::list_daemons()?;
        if daemons.is_empty() {
          println!("No daemons found.");
        } else {
          for d in &daemons {
            let status = if d.is_running { "running" } else { "stale" };
            println!(
              "[{}] pid={} dir={} socket={} version={}",
              status,
              d.contents.pid,
              d.contents.working_dir,
              d.contents.socket,
              d.contents.version,
            );
          }
        }
      }
      Some(("clean", _sub_m)) => {
        let count = lockfile::cleanup_all_stale()?;
        println!("Removed {} stale lock file(s).", count);
      }
      _ => {
        println!("Expected more arguments after `dk server`");
      }
    },
    Some((arg, _sub_m)) => {
      println!("Unknown: {}", arg);
    }
    None => {
      let paths = matches
        .get_many::<String>("files")
        .map(|p| p.collect::<Vec<_>>())
        .unwrap_or_default();

      if let Some(first) = paths.first() {
        // .lua
        if first.ends_with(".lua") {
          let src = std::fs::read_to_string(first)?;

          let lua = mlua::Lua::new();
          let cancel = tokio_util::sync::CancellationToken::new();
          lua.set_app_data(cancel.clone());
          lua
            .globals()
            .set("std", init_std(&lua).map_err(|e| anyhow!("{}", e))?)
            .map_err(|e| anyhow!("{}", e))?;

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
      } else {
        // No args: connect to daemon for current dir, starting it on
        // demand.
        let working_dir = std::env::current_dir()?;
        let (sender, receiver) =
          connect_client_socket::<CltToSrv, SrvToClt>(&working_dir, true)
            .await?;
        client_main(sender, receiver).await?;
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
