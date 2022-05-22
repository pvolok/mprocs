use crate::{
  config::{Config, ServerConfig},
  event::AppEvent,
};

pub async fn run_ctl(ctl: &str, config: &Config) -> anyhow::Result<()> {
  let event: AppEvent = serde_json::from_str(ctl)?;

  let socket = match &config.server {
    Some(ServerConfig::Tcp(addr)) => std::net::TcpStream::connect(addr)?,
    None => anyhow::bail!("Server address is not defined."),
  };

  serde_json::to_writer(socket, &event).unwrap();

  Ok(())
}
