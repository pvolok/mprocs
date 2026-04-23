use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail};
use clap::{Arg, Command};
use rquickjs::CatchResultExt;

use crate::mprocs::app::{ClientHandle, ClientId};
use crate::{
  client::client_main,
  daemon::{
    lockfile,
    receiver::MsgReceiver,
    sender::MsgSender,
    socket::{bind_server_socket, connect_client_socket},
  },
  dk_app::create_dk_app_task,
  js::js_vm::JsVm,
  kernel::{
    kernel::Kernel,
    kernel_message::{
      KernelCommand, KernelQuery, KernelQueryResponse, TaskContext,
    },
    task::{TaskCmd, TaskStatus},
    task_path::TaskPath,
  },
  lualib::init_std,
  protocol::{CltToSrv, DkRequest, DkResponse, DkTaskInfo, SrvToClt},
  server::server_message::ServerMessage,
  term::Size,
};

async fn run_server(working_dir: PathBuf) -> anyhow::Result<()> {
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
  let kernel = Kernel::new();
  let pc = kernel.context();

  let socket_path = lock_guard.socket_path().to_path_buf();
  let app_task_id = create_dk_app_task(&pc);
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
        Ok((sender, receiver)) => {
          last_client_id += 1;
          let client_id = ClientId(last_client_id);
          let app_sender = app_sender.clone();
          let pc = pc.clone();
          tokio::spawn(async move {
            dispatch_connection(client_id, app_sender, pc, sender, receiver)
              .await;
          });
        }
        Err(err) => {
          log::info!("Server socket accept error: {}", err);
          break;
        }
      }
    }
  });

  kernel.run().await;

  // lock_guard is dropped here, removing lock + socket files.
  drop(lock_guard);

  #[cfg(unix)]
  crate::process::unix_processes_waiter::UnixProcessesWaiter::uninit()?;

  Ok(())
}

/// Dispatch an accepted connection: RPC or TUI.
async fn dispatch_connection(
  client_id: ClientId,
  app_sender: crate::kernel::kernel_message::TaskSender,
  pc: TaskContext,
  mut sender: MsgSender<SrvToClt>,
  mut receiver: MsgReceiver<CltToSrv>,
) {
  let first_msg = receiver.recv().await;
  match first_msg {
    Some(Ok(CltToSrv::Rpc(req))) => {
      let resp = handle_rpc(&pc, req)
        .await
        .unwrap_or_else(|e| DkResponse::Error(e.to_string()));
      let _ = sender.send(SrvToClt::Rpc(resp)).await;
    }
    Some(Ok(CltToSrv::Init { width, height })) => {
      let handle =
        ClientHandle::create(client_id, sender, Size { width, height });
      match handle {
        Ok(handle) => {
          app_sender
            .send(TaskCmd::msg(ServerMessage::ClientConnected { handle }));
        }
        Err(err) => {
          log::error!("Client creation error: {:?}", err);
          return;
        }
      }
      // Forward subsequent messages to app.
      loop {
        let msg = if let Some(msg) = receiver.recv().await {
          msg
        } else {
          break;
        };
        match msg {
          Ok(msg) => {
            app_sender.send(TaskCmd::msg(ServerMessage::ClientMessage {
              client_id,
              msg,
            }));
          }
          Err(_err) => break,
        }
      }
      app_sender.send(TaskCmd::msg(ServerMessage::ClientDisconnected {
        client_id,
      }));
    }
    _ => {
      log::warn!("Unexpected first message from client");
    }
  }
}

async fn handle_rpc(
  pc: &TaskContext,
  req: DkRequest,
) -> anyhow::Result<DkResponse> {
  let response = match req {
    DkRequest::Ls { glob } => {
      let query = KernelQuery::ListTasks(glob);
      match pc.query(query).await? {
        KernelQueryResponse::TaskList(tasks) => {
          let items = tasks
            .into_iter()
            .map(|t| DkTaskInfo {
              path: t
                .path
                .map(|p| p.to_string())
                .unwrap_or_else(|| format!("<task:{}>", t.id.0)),
              status: match t.status {
                TaskStatus::Running => "running".to_string(),
                TaskStatus::Down => "down".to_string(),
              },
            })
            .collect();
          DkResponse::TaskList(items)
        }
        _ => DkResponse::Error("unexpected query response".to_string()),
      }
    }

    DkRequest::Start { path } => {
      let path = TaskPath::new(&path)?;
      pc.send_to_path(path, TaskCmd::Start);
      DkResponse::Ok
    }

    DkRequest::Stop { path } => {
      let path = TaskPath::new(&path)?;
      pc.send_to_path(path, TaskCmd::Stop);
      DkResponse::Ok
    }

    DkRequest::Kill { path } => {
      let path = TaskPath::new(&path)?;
      pc.send_to_path(path, TaskCmd::Kill);
      DkResponse::Ok
    }

    DkRequest::Restart { path } => {
      let path = TaskPath::new(&path)?;
      pc.send_to_path(path.clone(), TaskCmd::Stop);
      pc.send_to_path(path, TaskCmd::Start);
      DkResponse::Ok
    }

    DkRequest::Screen { path } => {
      let path = TaskPath::new(&path)?;
      let query = KernelQuery::GetScreen(path);
      match pc.query(query).await? {
        KernelQueryResponse::Screen(content) => DkResponse::Screen(content),
        _ => DkResponse::Error("unexpected query response".to_string()),
      }
    }

    DkRequest::Spawn { path, cmd, cwd } => {
      let task_path = TaskPath::new(&path)?;
      if cmd.is_empty() {
        bail!("cmd must not be empty".to_string());
      }
      let mut spec = crate::process::process_spec::ProcessSpec::from_argv(cmd);
      if let Some(cwd) = cwd {
        spec.cwd(cwd);
      } else if let Ok(cwd) = std::env::current_dir() {
        spec.cwd(cwd.to_string_lossy());
      }
      crate::task::proc_task::ProcTask::spawn(pc, task_path, spec);
      DkResponse::Ok
    }
  };
  Ok(response)
}

async fn rpc_request(
  working_dir: &Path,
  req: DkRequest,
  spawn_server: bool,
) -> anyhow::Result<DkResponse> {
  let (mut sender, mut receiver) =
    connect_client_socket::<CltToSrv, SrvToClt>(working_dir, spawn_server)
      .await?;
  sender.send(CltToSrv::Rpc(req)).await?;
  match receiver.recv().await {
    Some(Ok(SrvToClt::Rpc(resp))) => Ok(resp),
    Some(Ok(other)) => {
      anyhow::bail!("unexpected response: {:?}", other)
    }
    Some(Err(e)) => anyhow::bail!("decode error: {}", e),
    None => anyhow::bail!("connection closed without response"),
  }
}

pub async fn dekit_main() -> anyhow::Result<()> {
  println!("* Welcome to dekit — playground for future features *\n");

  let cmd = clap::command!()
    .subcommands([
      Command::new("attach"),
      Command::new("up"),
      Command::new("down"),
      Command::new("spawn")
        .about("Create and start a new task at the given path")
        .arg(
          Arg::new("path")
            .long("path")
            .required(true)
            .help("Task path (e.g. /services/web)"),
        )
        .arg(
          Arg::new("cwd")
            .long("cwd")
            .help("Working directory for the process"),
        )
        .arg(
          Arg::new("cmd")
            .required(true)
            .num_args(1..)
            .last(true)
            .help("Command to run"),
        ),
      Command::new("ls")
        .about("List tasks")
        .arg(Arg::new("glob").help("Optional glob pattern")),
      Command::new("start")
        .about("Start a stopped task")
        .arg(Arg::new("path").required(true).help("Task path")),
      Command::new("stop")
        .about("Gracefully stop a task")
        .arg(Arg::new("path").required(true).help("Task path")),
      Command::new("kill")
        .about("Force kill a task")
        .arg(Arg::new("path").required(true).help("Task path")),
      Command::new("restart")
        .about("Restart a task")
        .arg(Arg::new("path").required(true).help("Task path")),
      Command::new("screen")
        .about("Print the current screen of a task")
        .arg(Arg::new("path").required(true).help("Task path")),
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
    Some(("spawn", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let cwd = sub_m.get_one::<String>("cwd").cloned();
      let cmd: Vec<String> =
        sub_m.get_many::<String>("cmd").unwrap().cloned().collect();
      let resp =
        rpc_request(&working_dir, DkRequest::Spawn { path, cmd, cwd }, true)
          .await?;
      match resp {
        DkResponse::Ok => println!("Spawned."),
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("ls", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let glob = sub_m.get_one::<String>("glob").cloned();
      let resp =
        rpc_request(&working_dir, DkRequest::Ls { glob }, false).await?;
      match resp {
        DkResponse::TaskList(tasks) => {
          if tasks.is_empty() {
            println!("No tasks.");
          } else {
            for t in &tasks {
              println!("{}\t{}", t.path, t.status);
            }
          }
        }
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("start", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Start { path }, true).await?;
      match resp {
        DkResponse::Ok => println!("Started."),
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("stop", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Stop { path }, false).await?;
      match resp {
        DkResponse::Ok => println!("Stopped."),
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("kill", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Kill { path }, false).await?;
      match resp {
        DkResponse::Ok => println!("Killed."),
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("restart", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Restart { path }, true).await?;
      match resp {
        DkResponse::Ok => println!("Restarted."),
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("screen", sub_m)) => {
      let working_dir = std::env::current_dir()?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Screen { path }, false).await?;
      match resp {
        DkResponse::Screen(Some(content)) => {
          print!("{}", content);
          // Reset terminal attributes after printing
          print!("\x1b[0m\n");
        }
        DkResponse::Screen(None) => {
          eprintln!("No screen content for this task.");
        }
        DkResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
      }
    }
    Some(("up", _sub_m)) => {
      // let working_dir = std::env::current_dir()?;
      // TODO: load config, spawn default process set via RPC
      println!("up: not yet implemented");
    }
    Some(("down", _sub_m)) => {
      let working_dir = std::env::current_dir()?;
      lockfile::stop_daemon(&working_dir)?;
      println!("Daemon stopped.");
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
