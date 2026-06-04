use serde::{Deserialize, Serialize};

use super::{grid::Rect, key::KeyMods};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MouseEvent {
  pub kind: MouseEventKind,
  pub x: i32,
  pub y: i32,
  pub mods: KeyMods,
}

impl MouseEvent {
  pub fn translate(self, area: Rect) -> Self {
    let mut ret = self;
    ret.x -= area.x as i32;
    ret.y -= area.y as i32;
    ret
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MouseEventKind {
  Down(MouseButton),
  Up(MouseButton),
  Drag(MouseButton),
  Moved,
  ScrollDown,
  ScrollUp,
  ScrollLeft,
  ScrollRight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MouseButton {
  Left,
  Right,
  Middle,
}
