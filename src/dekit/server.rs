use std::path::PathBuf;

use anyhow::bail;

use crate::{
  console::{app::create_app_task, app_client::client_session},
  daemon::{lockfile, socket::bind_server_socket},
  ipc::{receiver::MsgReceiver, sender::MsgSender},
  kernel::{
    kernel::Kernel,
    kernel_message::{
      KernelCommand, KernelQuery, KernelQueryResponse, TaskContext,
    },
    task::{TaskCmd, TaskStatus},
    task_path::TaskPath,
  },
  protocol::{ClientId, CltToSrv, DkRequest, DkResponse, DkTaskInfo, SrvToClt},
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
            dispatch_connection(client_id, app_sender, pc, sender, receiver)
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
                TaskStatus::NotStarted => "not-started".to_string(),
                TaskStatus::Exited(code) => format!("exited:{}", code),
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
      pc.restart_path(path);
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
      crate::task::proc_task::spawn_proc_task(
        pc,
        Some(task_path),
        crate::task::proc_task::ProcTaskConfig::new(spec),
      );
      DkResponse::Ok
    }
  };
  Ok(response)
}
