use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Size {
  pub height: u16,
  pub width: u16,
}

#[derive(
  Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq, Serialize,
)]
pub enum CursorStyle {
  #[default]
  Default = 0,
  BlinkingBlock = 1,
  SteadyBlock = 2,
  BlinkingUnderline = 3,
  SteadyUnderline = 4,
  BlinkingBar = 5,
  SteadyBar = 6,
}

#[derive(Clone, Copy)]
pub struct Winsize {
  pub x: u16,
  pub y: u16,
  pub x_px: u16,
  pub y_px: u16,
}

#[cfg(unix)]
impl From<Winsize> for libc::winsize {
  fn from(value: Winsize) -> Self {
    libc::winsize {
      ws_row: value.y,
      ws_col: value.x,
      ws_xpixel: value.x_px,
      ws_ypixel: value.y_px,
    }
  }
}

#[cfg(unix)]
impl From<Winsize> for rustix::termios::Winsize {
  fn from(value: Winsize) -> Self {
    rustix::termios::Winsize {
      ws_row: value.y,
      ws_col: value.x,
      ws_xpixel: value.x_px,
      ws_ypixel: value.y_px,
    }
  }
}
