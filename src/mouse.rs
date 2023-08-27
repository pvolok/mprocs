use crossterm::event::{KeyModifiers, MouseEventKind};
use tui::prelude::Rect;

#[derive(Debug)]
pub struct MouseEvent {
  pub kind: MouseEventKind,
  pub x: i32,
  pub y: i32,
  pub mods: KeyModifiers,
}

impl MouseEvent {
  pub fn from_crossterm(event: crossterm::event::MouseEvent) -> Self {
    Self {
      kind: event.kind,
      x: event.column.into(),
      y: event.row.into(),
      mods: event.modifiers,
    }
  }

  pub fn translate(self, area: Rect) -> Self {
    let mut ret = self;
    ret.x -= area.x as i32;
    ret.y -= area.y as i32;
    ret
  }
}
