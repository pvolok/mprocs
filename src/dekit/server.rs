use std::path::PathBuf;

use anyhow::bail;

use crate::{
  console::{app::create_app_task, app_client::client_session},
  daemon::{lockfile, socket::bind_server_socket},
  ipc::{receiver::MsgReceiver, sender::MsgSender},
  kernel::{
    kernel::Kernel,
    kernel_message::{
      KernelCommand, KernelQuery, KernelQueryResponse, TaskContext, TaskInfo,
    },
    task::{TargetTask, TaskDef, TaskId, TaskState},
    task_path::TaskPath,
  },
  protocol::{
    ClientId, CltToSrv, DkRequest, DkResponse, DkTaskInfo, DkWhy, DkWhyDep,
    SrvToClt,
  },
  term::Size,
};

pub async fn run_server(
  working_dir: PathBuf,
  log_level: Option<&str>,
) -> anyhow::Result<()> {
  let (config, keymap, load_err) =
    match crate::config::config::Config::load_dir(&working_dir) {
      Ok(config) => {
        let keymap = config.keymap.build();
        (config, keymap, None)
      }
      Err(err) => {
        let config = crate::config::config::Config::make_default();
        let keymap = config.keymap.build();
        (config, keymap, Some(err))
      }
    };

  let _logger = crate::logging::init(crate::logging::Config {
    binary: "dk",
    cli_level: log_level,
    log_env: "DK_LOG",
    file_env: "DK_LOG_FILE",
    config_level: config.log.level.as_deref(),
    config_file: config.log.file.as_deref(),
    default_dir: Some(&working_dir),
  })?;

  if let Some(err) = load_err {
    log::warn!("Failed to load dekit config: {}", err);
  }

  // Create lock file and acquire exclusive flock.
  let lock_guard = lockfile::create_lock_file(&working_dir)?;
  log::info!("Lock file created for directory: {}", working_dir.display());

  #[cfg(unix)]
  crate::process::unix_processes_waiter::UnixProcessesWaiter::init()?;
  let kernel = Kernel::new();
  let pc = kernel.context();

  let socket_path = lock_guard.socket_path().to_path_buf();
  let app_task_id = create_app_task(config, keymap, &pc);
  let app_sender = pc.get_task_sender(app_task_id);

  // Umbrella target that wants every task spawned over RPC.
  let user_target_id = pc.register(
    TaskDef {
      pinned: true,
      path: TaskPath::new("/user").ok(),
      ..Default::default()
    },
    Box::new(|_| Box::new(TargetTask)),
  );

  tokio::spawn(async move {
    let mut last_client_id = 0;

    let mut server_socket = match bind_server_socket(&socket_path).await {
      Ok(server_socket) => {
        log::info!("Server is listening.");
        #[cfg(unix)]
        {
          server_socket
        }
        #[cfg(windows)]
        {
          let (sock, _addr) = server_socket;
          sock
        }
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
            dispatch_connection(
              client_id,
              app_sender,
              pc,
              user_target_id,
              sender,
              receiver,
            )
            .await;
          });
        }
        Err(err) => {
          log::debug!("Server socket accept error: {}", err);
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
  user_target_id: TaskId,
  mut sender: MsgSender<SrvToClt>,
  mut receiver: MsgReceiver<CltToSrv>,
) {
  let first_msg = receiver.recv().await;
  match first_msg {
    Some(Ok(CltToSrv::Rpc(req))) => {
      let resp = handle_rpc(&pc, user_target_id, req)
        .await
        .unwrap_or_else(|e| DkResponse::Error(e.to_string()));
      let _ = sender.send(SrvToClt::Rpc(resp)).await;
    }
    Some(Ok(CltToSrv::Init { width, height })) => {
      client_session(
        client_id,
        app_sender,
        Size { width, height },
        sender,
        receiver,
      )
      .await;
    }
    _ => {
      log::warn!("Unexpected first message from client");
    }
  }
}

fn state_str(state: TaskState) -> String {
  match state {
    TaskState::Idle => "idle".to_string(),
    TaskState::Starting => "starting".to_string(),
    TaskState::Running => "running".to_string(),
    TaskState::Ready => "ready".to_string(),
    TaskState::Stopping => "stopping".to_string(),
    TaskState::Backoff => "backoff".to_string(),
    TaskState::Done(info) => format!("done ({})", info),
    TaskState::Exited(info) => info.to_string(),
  }
}

async fn list_tasks(
  pc: &TaskContext,
  glob: Option<String>,
) -> anyhow::Result<Vec<TaskInfo>> {
  match pc.query(KernelQuery::ListTasks(glob)).await? {
    KernelQueryResponse::TaskList(tasks) => Ok(tasks),
    _ => bail!("unexpected query response"),
  }
}

async fn match_tasks(
  pc: &TaskContext,
  pattern: &str,
) -> anyhow::Result<Vec<TaskInfo>> {
  let tasks = list_tasks(pc, Some(pattern.to_string())).await?;
  if tasks.is_empty() {
    bail!("no tasks match '{}'", pattern);
  }
  Ok(tasks)
}

async fn handle_rpc(
  pc: &TaskContext,
  user_target_id: TaskId,
  req: DkRequest,
) -> anyhow::Result<DkResponse> {
  let response = match req {
    DkRequest::Ls { glob } => {
      let items = list_tasks(pc, glob)
        .await?
        .into_iter()
        .map(|t| DkTaskInfo {
          path: t
            .path
            .map(|p| p.to_string())
            .unwrap_or_else(|| format!("<task:{}>", t.id.0)),
          state: state_str(t.state),
        })
        .collect();
      DkResponse::TaskList(items)
    }

    DkRequest::Up => {
      let path = TaskPath::new("/autostart")?;
      match pc.query(KernelQuery::ResolvePath(path)).await? {
        KernelQueryResponse::ResolvedPath(Some(id)) => {
          pc.send(KernelCommand::Start(id));
          DkResponse::Ok
        }
        KernelQueryResponse::ResolvedPath(None) => {
          bail!("no autostart target")
        }
        _ => bail!("unexpected query response"),
      }
    }

    DkRequest::Start { pattern } => {
      for t in match_tasks(pc, &pattern).await? {
        pc.send(KernelCommand::Start(t.id));
      }
      DkResponse::Ok
    }

    DkRequest::Stop { pattern } => {
      for t in match_tasks(pc, &pattern).await? {
        pc.send(KernelCommand::Stop(t.id));
      }
      DkResponse::Ok
    }

    DkRequest::KeepDown { pattern } => {
      for t in match_tasks(pc, &pattern).await? {
        pc.send(KernelCommand::KeepDown(t.id));
      }
      DkResponse::Ok
    }

    DkRequest::Down { pattern } => {
      for t in match_tasks(pc, &pattern).await? {
        pc.send(KernelCommand::Down(t.id));
      }
      DkResponse::Ok
    }

    DkRequest::Kill { pattern } => {
      for t in match_tasks(pc, &pattern).await? {
        pc.send(KernelCommand::Kill(t.id));
      }
      DkResponse::Ok
    }

    DkRequest::Restart { pattern } => {
      for t in match_tasks(pc, &pattern).await? {
        pc.send(KernelCommand::Restart(t.id));
      }
      DkResponse::Ok
    }

    DkRequest::Why { path } => {
      let task_path = TaskPath::new(&path)?;
      match pc.query(KernelQuery::Explain(task_path)).await? {
        KernelQueryResponse::Explain(Some(explain)) => DkResponse::Why(DkWhy {
          path,
          state: state_str(explain.state),
          wanted: explain.wanted,
          supported: explain.supported,
          kept_down: explain.kept_down,
          pinned: explain.pinned,
          required_by: explain.required_by,
          deps: explain
            .deps
            .into_iter()
            .map(|d| DkWhyDep {
              path: d.name,
              state: state_str(d.state),
              wanted: d.wanted,
              satisfied: d.satisfied,
            })
            .collect(),
          attempts: explain.attempts,
        }),
        KernelQueryResponse::Explain(None) => {
          bail!("no task at '{}'", path)
        }
        _ => bail!("unexpected query response"),
      }
    }

    DkRequest::Screen { path } => {
      let path = TaskPath::new(&path)?;
      let query = KernelQuery::GetScreen(path);
      match pc.query(query).await? {
        KernelQueryResponse::Screen(content) => DkResponse::Screen(content),
        _ => DkResponse::Error("unexpected query response".to_string()),
      }
    }

    DkRequest::Shutdown => {
      pc.send(KernelCommand::Quit);
      DkResponse::Ok
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
      let id = crate::task::proc_task::spawn_proc_task(
        pc,
        Some(task_path),
        crate::task::proc_task::ProcTaskConfig::new(spec),
      );
      pc.send(KernelCommand::AddEdge {
        from: user_target_id,
        to: id,
      });
      DkResponse::Ok
    }
  };
  Ok(response)
}
