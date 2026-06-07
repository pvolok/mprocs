pub mod view;

use anyhow::bail;

use crate::cfg::{CfgCx, CfgNode, FromCfg};
use crate::mprocs::yaml_val::Val;
pub use crate::task::proc_task::StopSignal;
use crate::term::key::KeySpec;

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

impl FromCfg for StopSignal {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> anyhow::Result<Self> {
    StopSignal::from_val(&Val::new(node.raw())?).map_err(|err| node.error(err))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::term::key::{Key, KeyCode, KeyMods};

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
