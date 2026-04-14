#[cfg(unix)]
pub use self::unix::{bind_server_socket, connect_client_socket};
#[cfg(windows)]
pub use self::windows::{bind_server_socket, connect_client_socket};

#[cfg(unix)]
mod unix {
  use std::{fmt::Debug, path::Path, time::Duration};

  use serde::{Serialize, de::DeserializeOwned};
  use tokio::net::{UnixListener, UnixStream};

  use crate::daemon::{
    daemon::spawn_server_daemon,
    lockfile::{self, cleanup_stale, daemon_paths, read_lock_file},
    receiver::MsgReceiver,
    sender::MsgSender,
  };

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

    Ok(ServerSocket { listener })
  }

  pub struct ServerSocket {
    listener: UnixListener,
  }

  impl ServerSocket {
    pub async fn accept<
      S: Serialize + Debug + Send + 'static,
      R: DeserializeOwned + Send + 'static,
    >(
      &mut self,
    ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
      let (stream, _addr) = self.listener.accept().await?;
      let (read, write) = stream.into_split();
      let sender = MsgSender::new(write);
      let receiver = MsgReceiver::new(read);
      Ok((sender, receiver))
    }
  }

  async fn connect_to_daemon<
    S: Serialize + Debug + Send + 'static,
    R: DeserializeOwned + Send + 'static,
  >(
    lock_path: &Path,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
    let contents = read_lock_file(lock_path)
      .ok_or_else(|| anyhow::anyhow!("Failed to read daemon lock file."))?;
    let socket = UnixStream::connect(&contents.socket)
      .await
      .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;
    let (read, write) = socket.into_split();
    Ok((MsgSender::new(write), MsgReceiver::new(read)))
  }

  pub async fn connect_client_socket<
    S: Serialize + Debug + Send + 'static,
    R: DeserializeOwned + Send + 'static,
  >(
    working_dir: &Path,
    spawn_server: bool,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
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
      if let Some(contents) = read_lock_file(&lock_path) {
        if lockfile::is_daemon_alive(&lock_path) {
          let socket_path = &contents.socket;
          if let Ok(socket) = UnixStream::connect(socket_path).await {
            let (read, write) = socket.into_split();
            return Ok((MsgSender::new(write), MsgReceiver::new(read)));
          }
        }
      }
      if tokio::time::Instant::now() >= deadline {
        anyhow::bail!("Timed out waiting for daemon to start.");
      }
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
  }
}

#[cfg(windows)]
mod windows {
  use std::{fmt::Debug, path::Path, time::Duration};

  use serde::{Serialize, de::DeserializeOwned};
  use tokio::net::{TcpListener, TcpStream};

  use crate::daemon::{
    daemon::spawn_server_daemon,
    lockfile::{self, cleanup_stale, daemon_paths, read_lock_file},
    receiver::MsgReceiver,
    sender::MsgSender,
  };

  pub async fn bind_server_socket(
    _socket_path: &Path,
  ) -> anyhow::Result<(ServerSocket, String)> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?.to_string();
    log::info!("Listening on {}", addr);

    Ok((ServerSocket { listener }, addr))
  }

  pub struct ServerSocket {
    listener: TcpListener,
  }

  impl ServerSocket {
    pub async fn accept<
      S: Serialize + Debug + Send + 'static,
      R: DeserializeOwned + Send + 'static,
    >(
      &mut self,
    ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
      let (stream, _addr) = self.listener.accept().await?;
      let (read, write) = stream.into_split();
      let sender = MsgSender::new(write);
      let receiver = MsgReceiver::new(read);
      Ok((sender, receiver))
    }
  }

  async fn connect_to_daemon<
    S: Serialize + Debug + Send + 'static,
    R: DeserializeOwned + Send + 'static,
  >(
    lock_path: &Path,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
    let contents = read_lock_file(lock_path)
      .ok_or_else(|| anyhow::anyhow!("Failed to read daemon lock file."))?;
    let socket = TcpStream::connect(&contents.socket)
      .await
      .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;
    let (read, write) = socket.into_split();
    Ok((MsgSender::new(write), MsgReceiver::new(read)))
  }

  pub async fn connect_client_socket<
    S: Serialize + Debug + Send + 'static,
    R: DeserializeOwned + Send + 'static,
  >(
    working_dir: &Path,
    spawn_server: bool,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
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

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
      if let Some(contents) = read_lock_file(&lock_path) {
        if lockfile::is_daemon_alive(&lock_path) {
          let addr = &contents.socket;
          if let Ok(socket) = TcpStream::connect(addr).await {
            let (read, write) = socket.into_split();
            return Ok((MsgSender::new(write), MsgReceiver::new(read)));
          }
        }
      }
      if tokio::time::Instant::now() >= deadline {
        anyhow::bail!("Timed out waiting for daemon to start.");
      }
      tokio::time::sleep(Duration::from_millis(50)).await;
    }
  }
}
