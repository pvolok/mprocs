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
