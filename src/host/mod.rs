mod daemon;

use std::{path::PathBuf, time::Duration};

use interprocess::local_socket::tokio::{
  LocalSocketListener, LocalSocketStream,
};

use crate::error::ResultLogger;

use self::daemon::spawn_server_daemon;

fn get_socket_path() -> PathBuf {
  let mut path = std::env::temp_dir();
  path.push("dekit.sock");
  path
}

pub fn create_server_socket() -> anyhow::Result<ServerSocket> {
  let path = get_socket_path();

  let bind = || LocalSocketListener::bind(path.clone());
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
  listener: LocalSocketListener,
}

impl Drop for ServerSocket {
  fn drop(&mut self) {
    std::fs::remove_file(&self.path).log_ignore();
  }
}

impl ServerSocket {
  pub fn listener(&mut self) -> &mut LocalSocketListener {
    &mut self.listener
  }
}

pub async fn connect_client_socket(
  mut spawn_server: bool,
) -> anyhow::Result<LocalSocketStream> {
  let path = get_socket_path();
  loop {
    match LocalSocketStream::connect(path.clone()).await {
      Ok(socket) => return Ok(socket),
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
