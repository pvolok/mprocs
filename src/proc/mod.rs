mod inst;
pub mod msg;
pub mod proc;
pub mod view;

use std::fmt::Debug;

use anyhow::bail;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tui::layout::Rect;

use crate::kernel::proc::ProcId;
use crate::key::Key;
use crate::vt100::TermReplySender;
use crate::yaml_val::Val;

use self::msg::ProcEvent;

#[derive(Clone, Debug, Default)]
pub enum StopSignal {
  SIGINT,
  #[default]
  SIGTERM,
  SIGKILL,
  SendKeys(Vec<Key>),
  HardKill,
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

impl Size {
  fn new(rect: Rect) -> Size {
    Size {
      width: rect.width.max(3),
      height: rect.height.max(3),
    }
  }
}

#[allow(clippy::large_enum_variant)]
pub enum CopyMode {
  None(Option<Pos>),
  Active(crate::vt100::Screen<ReplySender>, Pos, Option<Pos>),
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

#[derive(Clone)]
pub struct ReplySender {
  proc_id: ProcId,
  sender: UnboundedSender<ProcEvent>,
}

impl TermReplySender for ReplySender {
  fn reply(&self, s: CompactString) {
    let _ = self.sender.send(ProcEvent::TermReply(s));
  }
}
