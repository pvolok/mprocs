use std::{path::Path, time::Duration};

use crate::daemon::{
  lockfile::{self, cleanup_stale, daemon_paths, read_lock_file},
  spawn::spawn_server_daemon,
};
use crate::protocol::{ConnReceiver, ConnSender};

pub async fn connect_client_socket(
  working_dir: &Path,
  spawn_server: bool,
) -> anyhow::Result<(ConnSender, ConnReceiver)> {
  let (lock_path, _socket_path) = daemon_paths(working_dir)?;

  let daemon_running = match read_lock_file(&lock_path) {
    Some(_) if lockfile::is_daemon_alive(&lock_path) => true,
    Some(_) => {
      let _ = cleanup_stale(working_dir);
      false
    }
    None => false,
  };

  if !daemon_running {
    if spawn_server {
      spawn_server_daemon(working_dir)?;
    } else {
      anyhow::bail!("Daemon is not running. Start it with `dk up`.");
    }
  }

  if daemon_running {
    return connect_to_daemon(&lock_path).await;
  }

  let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
  loop {
    if read_lock_file(&lock_path).is_some()
      && lockfile::is_daemon_alive(&lock_path)
      && let Ok(conn) = connect_to_daemon(&lock_path).await
    {
      return Ok(conn);
    }
    if tokio::time::Instant::now() >= deadline {
      anyhow::bail!("Timed out waiting for daemon to start.");
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
}

async fn connect_to_daemon(
  lock_path: &Path,
) -> anyhow::Result<(ConnSender, ConnReceiver)> {
  let contents = read_lock_file(lock_path)
    .ok_or_else(|| anyhow::anyhow!("Failed to read daemon lock file."))?;
  connect_socket(&contents.socket).await
}

#[cfg(unix)]
pub use self::unix::{bind_server_socket, connect_socket};
#[cfg(windows)]
pub use self::windows::{bind_server_socket, connect_socket};

#[cfg(unix)]
mod unix {
  use std::path::Path;

  use tokio::net::{UnixListener, UnixStream};

  use crate::protocol::{ConnReceiver, ConnSender};

  pub async fn bind_server_socket(
    socket_path: &Path,
  ) -> anyhow::Result<ServerSocket> {
    let bind = || UnixListener::bind(socket_path);
    let listener = match bind() {
      Ok(listener) => listener,
      Err(err) => match err.kind() {
        std::io::ErrorKind::AddrInUse => {
          std::fs::remove_file(socket_path)?;
          bind()?
        }
        _ => return Err(err.into()),
      },
    };

    // Only the owner may talk to the daemon.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
      socket_path,
      std::fs::Permissions::from_mode(0o600),
    )?;

    Ok(ServerSocket { listener })
  }

  pub struct ServerSocket {
    listener: UnixListener,
  }

  impl ServerSocket {
    pub async fn accept(
      &mut self,
    ) -> anyhow::Result<(ConnSender, ConnReceiver)> {
      let (stream, _addr) = self.listener.accept().await?;
      let (read, write) = stream.into_split();
      Ok((ConnSender::new(write), ConnReceiver::new(read)))
    }
  }

  pub async fn connect_socket(
    socket: &str,
  ) -> anyhow::Result<(ConnSender, ConnReceiver)> {
    let stream = UnixStream::connect(socket)
      .await
      .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;
    let (read, write) = stream.into_split();
    Ok((ConnSender::new(write), ConnReceiver::new(read)))
  }
}

#[cfg(windows)]
mod windows {
  use std::{path::Path, time::Duration};

  use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeServer, ServerOptions,
  };

  use crate::protocol::{ConnReceiver, ConnSender};

  // ERROR_PIPE_BUSY: all pipe instances are taken; retry shortly.
  const PIPE_BUSY: i32 = 231;

  pub async fn bind_server_socket(
    socket_path: &Path,
  ) -> anyhow::Result<ServerSocket> {
    let pipe_name = socket_path.to_string_lossy().into_owned();
    let next = ServerOptions::new()
      .first_pipe_instance(true)
      .create(&pipe_name)?;
    Ok(ServerSocket { pipe_name, next })
  }

  pub struct ServerSocket {
    pipe_name: String,
    next: NamedPipeServer,
  }

  impl ServerSocket {
    pub async fn accept(
      &mut self,
    ) -> anyhow::Result<(ConnSender, ConnReceiver)> {
      self.next.connect().await?;
      let connected = std::mem::replace(
        &mut self.next,
        ServerOptions::new().create(&self.pipe_name)?,
      );
      let (read, write) = tokio::io::split(connected);
      Ok((ConnSender::new(write), ConnReceiver::new(read)))
    }
  }

  pub async fn connect_socket(
    socket: &str,
  ) -> anyhow::Result<(ConnSender, ConnReceiver)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let pipe = loop {
      match ClientOptions::new().open(socket) {
        Ok(pipe) => break pipe,
        Err(err) if err.raw_os_error() == Some(PIPE_BUSY) => {
          if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("Failed to connect to daemon: pipe is busy");
          }
          tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(err) => {
          anyhow::bail!("Failed to connect to daemon: {}", err);
        }
      }
    };
    let (read, write) = tokio::io::split(pipe);
    Ok((ConnSender::new(write), ConnReceiver::new(read)))
  }
}
