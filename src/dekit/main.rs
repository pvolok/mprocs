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

fn print_task_list(resp: DkResponse, json: bool) -> anyhow::Result<()> {
  match resp {
    DkResponse::TaskList(tasks) => {
      if json {
        println!("{}", serde_json::to_string(&tasks)?);
      } else if tasks.is_empty() {
        println!("No tasks.");
      } else {
        for t in &tasks {
          println!("{}\t{}", t.path, t.status);
        }
      }
      Ok(())
    }
    DkResponse::Error(e) => anyhow::bail!("{}", e),
    _ => anyhow::bail!("unexpected response from daemon"),
  }
}

fn resolve_working_dir(matches: &clap::ArgMatches) -> anyhow::Result<PathBuf> {
  match matches.get_one::<String>("chdir") {
    Some(dir) => std::fs::canonicalize(dir)
      .map_err(|e| anyhow!("invalid --chdir `{}`: {}", dir, e)),
    None => Ok(std::env::current_dir()?),
  }
}

async fn shutdown_daemon(working_dir: &Path) -> anyhow::Result<()> {
  match lockfile::get_daemon_status(working_dir)? {
    None => anyhow::bail!("No daemon found for this directory"),
    Some(info) if !info.is_running => {
      lockfile::cleanup_stale(working_dir)?;
      anyhow::bail!("Daemon is not running (stale lock file cleaned up)");
    }
    Some(_) => {}
  }

  let _ = rpc_request(working_dir, DkRequest::Shutdown, false).await;

  for _ in 0..50 {
    match lockfile::get_daemon_status(working_dir)? {
      None => return Ok(()),
      Some(info) if !info.is_running => {
        lockfile::cleanup_stale(working_dir)?;
        return Ok(());
      }
      Some(_) => {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await
      }
    }
  }

  lockfile::stop_daemon(working_dir)?;
  Ok(())
}

fn daemon_json(info: Option<&lockfile::DaemonInfo>) -> serde_json::Value {
  match info {
    Some(info) => serde_json::json!({
      "running": info.is_running,
      "pid": info.contents.pid,
      "dir": info.contents.working_dir,
      "socket": info.contents.socket,
      "version": info.contents.version,
    }),
    None => serde_json::Value::Null,
  }
}

async fn wait_for_daemon(working_dir: &Path) -> anyhow::Result<()> {
  for _ in 0..50 {
    if let Some(info) = lockfile::get_daemon_status(working_dir)? {
      if info.is_running {
        return Ok(());
      }
    }
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
  }
  anyhow::bail!("daemon did not come up within 2s");
}

fn report_ok(resp: DkResponse, ok_msg: &str) -> anyhow::Result<()> {
  match resp {
    DkResponse::Ok => {
      println!("{}", ok_msg);
      Ok(())
    }
    DkResponse::Error(e) => anyhow::bail!("{}", e),
    _ => anyhow::bail!("unexpected response from daemon"),
  }
}

pub async fn dekit_main() -> anyhow::Result<()> {
  let cmd = clap::command!()
    .subcommands([
      Command::new("attach") ,
      Command::new("up"),
      Command::new("down"),
      Command::new("spawn")
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
        .about("Start a task")
        .arg(Arg::new("path").required(true).help("Task path")),
      Command::new("stop")
        .about("Stop a task")
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
      Command::new("server")
        .about("Manage the background server")
        .subcommands([
        Command::new("run")
          .about("Run the daemon in the foreground")
          .arg(
            Arg::new("dir")
              .long("dir")
              .required(true)
              .help("Working directory this server manages"),
          )
          .arg(
            Arg::new("log-level")
              .long("log-level")
              .help("Diagnostic log level (off|error|warn|info|debug|trace, or env_logger spec). Falls back to $DK_LOG, $RUST_LOG, then 'error' (release) or 'trace' (debug)."),
          ),
        Command::new("start")
          .about("Start the server for the current directory"),
        Command::new("stop").about("Stop the server for the current directory"),
        Command::new("status")
          .about("Show server status for the current directory"),
        Command::new("list").about("List all server on this machine"),
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
      Arg::new("chdir")
        .long("chdir")
        .short('C')
        .global(true)
        .help("Directory whose server to talk to (default: current dir)"),
    )
    .arg(
      Arg::new("json")
        .long("json")
        .global(true)
        .action(clap::ArgAction::SetTrue)
        .help("Emit machine-readable JSON instead of text"),
    )
    .arg(
      Arg::new("files")
        .action(clap::ArgAction::Append)
        .trailing_var_arg(true)
        .help("A .js script to run; with no command, launch the TUI"),
    );
  let matches = cmd.get_matches();

  if let Some(("mprocs", sub_m)) = matches.subcommand() {
    let args: Vec<String> = sub_m
      .get_many::<String>("args")
      .map(|vals| vals.cloned().collect())
      .unwrap_or_default();
    let mut argv = vec!["mprocs".to_string()];
    argv.extend(args);
    return crate::mprocs::mprocs::run_app(argv).await;
  }

  match matches.subcommand() {
    Some(("attach", _sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let (sender, receiver) =
        connect_client_socket::<CltToSrv, SrvToClt>(&working_dir, false)
          .await?;
      client_main(sender, receiver).await?;
    }
    Some(("spawn", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let cwd = sub_m.get_one::<String>("cwd").cloned();
      let cmd: Vec<String> =
        sub_m.get_many::<String>("cmd").unwrap().cloned().collect();
      let resp =
        rpc_request(&working_dir, DkRequest::Spawn { path, cmd, cwd }, true)
          .await?;
      report_ok(resp, "Spawned.")?;
    }
    Some(("ls", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let glob = sub_m.get_one::<String>("glob").cloned();
      let resp =
        rpc_request(&working_dir, DkRequest::Ls { glob }, false).await?;
      print_task_list(resp, matches.get_flag("json"))?;
    }
    Some(("start", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Start { path }, true).await?;
      report_ok(resp, "Started.")?;
    }
    Some(("stop", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Stop { path }, false).await?;
      report_ok(resp, "Stopped.")?;
    }
    Some(("kill", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Kill { path }, false).await?;
      report_ok(resp, "Killed.")?;
    }
    Some(("restart", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let path = sub_m.get_one::<String>("path").unwrap().clone();
      let resp =
        rpc_request(&working_dir, DkRequest::Restart { path }, true).await?;
      report_ok(resp, "Restarted.")?;
    }
    Some(("screen", sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
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
          anyhow::bail!("no screen content for this task");
        }
        DkResponse::Error(e) => anyhow::bail!("{}", e),
        _ => anyhow::bail!("unexpected response from daemon"),
      }
    }
    Some(("up", _sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      let resp =
        rpc_request(&working_dir, DkRequest::Ls { glob: None }, true).await?;
      print_task_list(resp, matches.get_flag("json"))?;
    }
    Some(("down", _sub_m)) => {
      let working_dir = resolve_working_dir(&matches)?;
      shutdown_daemon(&working_dir).await?;
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
        let working_dir = resolve_working_dir(&matches)?;
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
        wait_for_daemon(&working_dir).await?;
        println!("Daemon started for {}.", working_dir.display());
      }
      Some(("stop", _sub_m)) => {
        let working_dir = resolve_working_dir(&matches)?;
        shutdown_daemon(&working_dir).await?;
        println!("Daemon stopped.");
      }
      Some(("status", _sub_m)) => {
        let working_dir = resolve_working_dir(&matches)?;
        let info = lockfile::get_daemon_status(&working_dir)?;
        if matches.get_flag("json") {
          println!("{}", serde_json::to_string(&daemon_json(info.as_ref()))?);
        } else {
          match info {
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
      }
      Some(("list", _sub_m)) => {
        let daemons = lockfile::list_daemons()?;
        if matches.get_flag("json") {
          let arr: Vec<_> =
            daemons.iter().map(|d| daemon_json(Some(d))).collect();
          println!("{}", serde_json::to_string(&arr)?);
        } else if daemons.is_empty() {
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
        anyhow::bail!(
          "expected a subcommand after `dk server` (run, start, stop, status, list, clean)"
        );
      }
    },
    Some((arg, _sub_m)) => {
      anyhow::bail!("unknown command: {}", arg);
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
          let root = vm
            .eval_file(Path::new(first.as_str()), src.as_bytes())
            .await?;

          rquickjs::async_with!(vm.context => |ctx| {
            run_module_main(&ctx, &root).await
          })
          .await?;
        } else {
          anyhow::bail!(
            "unknown command or unsupported file: `{}` (expected a subcommand or a .js script)",
            first
          );
        }
      } else {
        // No args: connect to daemon for current dir, starting it on
        // demand.
        let working_dir = resolve_working_dir(&matches)?;
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
