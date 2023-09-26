#[cfg(unix)]
pub use self::unix::{bind_server_socket, connect_client_socket};

#[cfg(unix)]
mod unix {
  use std::{fmt::Debug, path::PathBuf, time::Duration};

  use serde::{de::DeserializeOwned, Serialize};
  use tokio::net::{UnixListener, UnixStream};

  use crate::{
    error::ResultLogger,
    host::{
      daemon::spawn_server_daemon, receiver::MsgReceiver, sender::MsgSender,
    },
  };

  fn get_socket_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("dekit.sock");
    path
  }

  pub fn bind_server_socket() -> anyhow::Result<ServerSocket> {
    let path = get_socket_path();

    let bind = || UnixListener::bind(&path);
    let listener = match bind() {
      Ok(listener) => listener,
      Err(err) => match err.kind() {
        std::io::ErrorKind::AddrInUse => {
          std::fs::remove_file(&path)?;
          bind()?
        }
        _ => return Err(err.into()),
      },
    };

    Ok(ServerSocket { path, listener })
  }

  pub struct ServerSocket {
    path: PathBuf,
    listener: UnixListener,
  }

  impl Drop for ServerSocket {
    fn drop(&mut self) {
      std::fs::remove_file(&self.path).log_ignore();
    }
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
    mut spawn_server: bool,
  ) -> anyhow::Result<(MsgSender<S>, MsgReceiver<R>)> {
    let path = get_socket_path();
    loop {
      match UnixStream::connect(&path).await {
        Ok(socket) => {
          let (read, write) = socket.into_split();
          let sender = MsgSender::new(write);
          let receiver = MsgReceiver::new(read);
          return Ok((sender, receiver));
        }
        Err(err) => {
          match err.kind() {
            std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused => {
              // ConnectionRefused: Socket exists, but no process is listening.

              if spawn_server {
                spawn_server = false;
                spawn_server_daemon()?;
              }
            }
            _ => (),
          }
          tokio::time::sleep(Duration::from_millis(20)).await;
        }
      }
    }
  }
}
