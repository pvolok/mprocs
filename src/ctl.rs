use serde_yaml::Value;

use crate::{
  config::{Config, ServerConfig},
  event::AppEvent,
};

pub async fn run_ctl(ctl: &str, config: &Config) -> anyhow::Result<()> {
  let event: AppEvent = match serde_yaml::from_str(ctl) {
    Ok(event) => event,
    Err(err) => {
      let val: Value = serde_yaml::from_str(ctl)?;
      println!(
        "Remote command parsed as:\n{}",
        serde_yaml::to_string(&val)?
      );
      return Err(err.into());
    }
  };

  let socket = match &config.server {
    Some(ServerConfig::Tcp(addr)) => std::net::TcpStream::connect(addr)?,
    None => anyhow::bail!("Server address is not defined."),
  };

  serde_yaml::to_writer(socket, &event).unwrap();

  Ok(())
}
