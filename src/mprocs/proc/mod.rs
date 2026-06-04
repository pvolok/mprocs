mod inst;
pub mod msg;
pub mod proc;
pub mod view;

use std::fmt::Debug;

use anyhow::bail;

use crate::mprocs::yaml_val::Val;
use crate::term::key::{Key, KeySpec};

#[derive(Clone, Debug, Default)]
pub enum StopSignal {
  SIGINT,
  #[default]
  SIGTERM,
  SIGKILL,
  SendKeys(Vec<Key>),
  HardKill,
  /// Run a shell command as the stop action. Useful for tools like
  /// `podman compose` that don't reliably respond to signals but do have
  /// an explicit teardown command (e.g. `podman compose down`). The main
  /// process is expected to exit on its own once the stop command
  /// completes (e.g. `compose up` exits when containers go away).
  Cmd(String),
}

impl StopSignal {
  pub fn from_val(val: &Val) -> anyhow::Result<Self> {
    match val.raw() {
      serde_yaml::Value::String(str) => match str.as_str() {
        "SIGINT" => return Ok(Self::SIGINT),
        "SIGTERM" => return Ok(Self::SIGTERM),
        "SIGKILL" => return Ok(Self::SIGKILL),
        "hard-kill" => return Ok(Self::HardKill),
        _ => (),
      },
      serde_yaml::Value::Mapping(map) => {
        if map.len() == 1 {
          if let Some(keys) = map.get("send-keys") {
            let keys: Vec<KeySpec> = serde_yaml::from_value(keys.clone())?;
            let keys = keys.into_iter().map(KeySpec::key).collect();
            return Ok(Self::SendKeys(keys));
          }
          if let Some(cmd) = map.get("cmd") {
            if let serde_yaml::Value::String(shell) = cmd {
              return Ok(Self::Cmd(shell.clone()));
            }
            bail!("Expected 'cmd' to be a string");
          }
        }
      }
      _ => (),
    }
    bail!("Unexpected 'stop' value: {:?}.", val.raw());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::term::key::{KeyCode, KeyMods};

  #[test]
  fn stop_signal_send_keys_uses_key_specs() {
    let raw: serde_yaml::Value = serde_yaml::from_str(
      "send-keys:\n  - <C-a>\n  - <F13>\n  - <MediaPlayPause>\n",
    )
    .unwrap();
    let val = Val::new(&raw).unwrap();

    let keys = match StopSignal::from_val(&val).unwrap() {
      StopSignal::SendKeys(keys) => keys,
      other => panic!("Expected SendKeys, got {other:?}"),
    };

    assert_eq!(
      keys,
      vec![
        Key::new(KeyCode::Char('a'), KeyMods::CONTROL),
        Key::new(KeyCode::F(13), KeyMods::NONE),
        Key::new(
          KeyCode::Media(crate::term::key::MediaKeyCode::PlayPause),
          KeyMods::NONE,
        ),
      ]
    );
  }
}

#[derive(Clone)]
pub struct Size {
  width: u16,
  height: u16,
}
