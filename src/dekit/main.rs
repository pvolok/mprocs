use std::path::{Path, PathBuf};

use anyhow::anyhow;
use clap::{Arg, Command};
use rquickjs::CatchResultExt;

use crate::{
  attach_client::client_main,
  daemon::{
    lockfile, socket::connect_client_socket, spawn::spawn_server_daemon,
  },
  dekit::{rpc_client::rpc_request, server::run_server},
  js::js_vm::JsVm,
  protocol::{CltToSrv, DkRequest, DkResponse, SrvToClt},
};

fn print_task_list(resp: DkResponse) {
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

pub async fn dekit_main() -> anyhow::Result<()> {
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
        Command::new("run")
          .arg(
            Arg::new("dir")
              .long("dir")
              .required(true)
              .help("Working directory this daemon manages"),
          )
          .arg(
            Arg::new("log-level")
              .long("log-level")
              .help("Diagnostic log level (off|error|warn|info|debug|trace, or env_logger spec). Falls back to $DK_LOG, $RUST_LOG, then 'error' (release) or 'trace' (debug)."),
          ),
        Command::new("start")
          .about("Start the daemon for the current directory"),
        Command::new("stop").about("Stop the daemon for the current directory"),
        Command::new("status")
          .about("Show daemon status for the current directory"),
        Command::new("list").about("List all daemons on this machine"),
        Command::new("clean").about("Remove stale lock files"),
      ]),
      Command::new("mprocs")
        .about("Run the legacy mprocs CLI (mprocs.yaml, --ctl, etc.)")
        .disable_help_flag(true)
        .arg(
          Arg::new("args")
            .num_args(0..)
            .trailing_var_arg(true)
            .allow_hyphen_values(true),
        ),
    ])
    .arg(
      Arg::new("files")
        .action(clap::ArgAction::Append)
        .trailing_var_arg(true),
    );
  let matches = cmd.get_matches();

  if let Some(("mprocs", sub_m)) = matches.subcommand() {
    let args: Vec<String> = sub_m
      .get_many::<String>("args")
      .map(|vals| vals.cloned().collect())
      .unwrap_or_default();
    let mut argv = vec!["mprocs".to_string()];
    argv.extend(args);
    return match crate::mprocs::mprocs::run_app(argv).await {
      Ok(()) => Ok(()),
      Err(err) => {
        eprintln!("Error: {:?}", err);
        Ok(())
      }
    };
  }

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
      print_task_list(resp);
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
      let working_dir = std::env::current_dir()?;
      let resp =
        rpc_request(&working_dir, DkRequest::Ls { glob: None }, true).await?;
      print_task_list(resp);
    }
    Some(("down", _sub_m)) => {
      let working_dir = std::env::current_dir()?;
      lockfile::stop_daemon(&working_dir)?;
      println!("Daemon stopped.");
    }
    Some(("server", sub_m)) => match sub_m.subcommand() {
      Some(("run", run_m)) => {
        let dir = run_m.get_one::<String>("dir").unwrap();
        let log_level =
          run_m.get_one::<String>("log-level").map(String::as_str);
        run_server(PathBuf::from(dir), log_level).await?;
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
        spawn_server_daemon(&working_dir)?;
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
        // .js
        if first.ends_with(".js") {
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
