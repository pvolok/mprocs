#[cfg(unix)]
pub use self::unix::{bind_server_socket, connect_client_socket};
#[cfg(windows)]
pub use self::windows::{bind_server_socket, connect_client_socket};

#[cfg(unix)]
mod unix {
  use std::{fmt::Debug, path::Path, time::Duration};

  use serde::{de::DeserializeOwned, Serialize};
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

  pub async fn connect_client_socket<
    S: Serialize + Debug + Send + 'static,
    R: DeserializeOwned + Send + 'static,
  >(
    working_dir: &Path,
    mut spawn_server: bool,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
    let (lock_path, _socket_path) = daemon_paths(working_dir)?;

    loop {
      // Try to read the lock file to find the socket.
      if let Some(contents) = read_lock_file(&lock_path) {
        if lockfile::is_daemon_alive(&lock_path) {
          let socket_path = &contents.socket;
          match UnixStream::connect(socket_path).await {
            Ok(socket) => {
              let (read, write) = socket.into_split();
              let sender = MsgSender::new(write);
              let receiver = MsgReceiver::new(read);
              return Ok((sender, receiver));
            }
            Err(err) => {
              match err.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                  // Daemon holds flock but socket not ready yet; wait.
                }
                _ => (),
              }
            }
          }
          tokio::time::sleep(Duration::from_millis(20)).await;
          continue;
        } else {
          // Stale lock file.
          let _ = cleanup_stale(working_dir);
        }
      }

      // No daemon running.
      if spawn_server {
        spawn_server = false;
        spawn_server_daemon(working_dir)?;
      }
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
  }
}

#[cfg(windows)]
mod windows {
  use std::{fmt::Debug, path::Path, time::Duration};

  use serde::{de::DeserializeOwned, Serialize};
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

  pub async fn connect_client_socket<
    S: Serialize + Debug + Send + 'static,
    R: DeserializeOwned + Send + 'static,
  >(
    working_dir: &Path,
    mut spawn_server: bool,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
    let (lock_path, _socket_path) = daemon_paths(working_dir)?;

    loop {
      if let Some(contents) = read_lock_file(&lock_path) {
        if lockfile::is_daemon_alive(&lock_path) {
          let addr = &contents.socket;
          match TcpStream::connect(addr).await {
            Ok(socket) => {
              let (read, write) = socket.into_split();
              let sender = MsgSender::new(write);
              let receiver = MsgReceiver::new(read);
              return Ok((sender, receiver));
            }
            Err(err) => {
              match err.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                  // Daemon holds flock but socket not ready yet.
                }
                _ => (),
              }
            }
          }
          tokio::time::sleep(Duration::from_millis(50)).await;
          continue;
        } else {
          let _ = cleanup_stale(working_dir);
        }
      }

      if spawn_server {
        spawn_server = false;
        spawn_server_daemon(working_dir)?;
      }
      tokio::time::sleep(Duration::from_millis(50)).await;
    }
  }
}
