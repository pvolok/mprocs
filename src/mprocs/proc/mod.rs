mod inst;
pub mod msg;
pub mod proc;
pub mod view;

use std::fmt::Debug;

use anyhow::bail;
use serde::{Deserialize, Serialize};

use crate::mprocs::yaml_val::Val;
use crate::term::key::Key;

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
            let keys: Vec<Key> = serde_yaml::from_value(keys.clone())?;
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

#[derive(Clone)]
pub struct Size {
  width: u16,
  height: u16,
}

#[allow(clippy::large_enum_variant)]
pub enum CopyMode {
  None(Option<Pos>),
  Active(crate::term::Screen, Pos, Option<Pos>),
}

impl Default for CopyMode {
  fn default() -> Self {
    CopyMode::None(None)
  }
}

#[derive(
  Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize,
)]
pub struct Pos {
  pub y: i32,
  pub x: i32,
}

impl Pos {
  pub fn to_low_high<'a>(a: &'a Self, b: &'a Self) -> (&'a Self, &'a Self) {
    if a.y < b.y || a.y == b.y && a.x < b.x {
      (a, b)
    } else {
      (b, a)
    }
  }

  pub fn within(start: &Self, end: &Self, target: &Self) -> bool {
    let y = target.y;
    let x = target.x;
    let (low, high) = Pos::to_low_high(start, end);

    if y > low.y {
      y < high.y || y == high.y && x <= high.x
    } else if y == low.y {
      if y < high.y {
        x >= low.x
      } else if y == high.y {
        x >= low.x && x <= high.x
      } else {
        false
      }
    } else {
      false
    }
  }
}
