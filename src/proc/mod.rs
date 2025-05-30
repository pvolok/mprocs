pub mod handle;
mod inst;
pub mod msg;
mod proc;

use std::fmt::Debug;

use anyhow::bail;
use compact_str::CompactString;
use handle::ProcHandle;
use proc::Proc;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tui::layout::Rect;

use crate::config::ProcConfig;
use crate::key::Key;
use crate::mouse::MouseEvent;
use crate::vt100::TermReplySender;
use crate::yaml_val::Val;

use self::msg::ProcEvent;

pub fn create_proc(
  name: String,
  cfg: &ProcConfig,
  tx: UnboundedSender<(usize, ProcEvent)>,
  size: Rect,
) -> ProcHandle {
  let proc = Proc::new(cfg, tx, size);
  ProcHandle::from_proc(name, proc, cfg.autorestart)
}

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

fn translate_mouse_pos(event: &MouseEvent, scrollback: usize) -> Pos {
  Pos {
    y: event.y - scrollback as i32,
    x: event.x,
  }
}

#[derive(Clone)]
struct Size {
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

pub enum CopyMode {
  None(Option<Pos>),
  Start(crate::vt100::Screen<ReplySender>, Pos),
  Range(crate::vt100::Screen<ReplySender>, Pos, Pos),
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
    if a.y > b.y {
      return (b, a);
    } else if a.y == b.y && a.x > b.x {
      return (b, a);
    }
    (a, b)
  }

  pub fn within(start: &Self, end: &Self, target: &Self) -> bool {
    let y = target.y;
    let x = target.x;
    let (low, high) = Pos::to_low_high(start, end);

    if y > low.y {
      if y < high.y {
        true
      } else if y == high.y && x <= high.x {
        true
      } else {
        false
      }
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
  proc_id: usize,
  sender: UnboundedSender<(usize, ProcEvent)>,
}

impl TermReplySender for ReplySender {
  fn reply(&self, s: CompactString) {
    let _ = self.sender.send((self.proc_id, ProcEvent::TermReply(s)));
  }
}
