/// Represents a foreground or background color for cells.
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Color {
  /// The default terminal color.
  Default,

  /// An indexed terminal color.
  Idx(u8),

  /// An RGB terminal color. The parameters are (red, green, blue).
  Rgb(u8, u8, u8),
}

#[allow(dead_code)]
impl Color {
  pub const BLACK: Self = Color::Idx(0);
  pub const RED: Self = Color::Idx(1);
  pub const GREEN: Self = Color::Idx(2);
  pub const YELLOW: Self = Color::Idx(3);
  pub const BLUE: Self = Color::Idx(4);
  pub const MAGENTA: Self = Color::Idx(5);
  pub const CYAN: Self = Color::Idx(6);
  pub const WHITE: Self = Color::Idx(7);

  pub const BRIGHT_BLACK: Self = Color::Idx(8);
  pub const BRIGHT_RED: Self = Color::Idx(9);
  pub const BRIGHT_GREEN: Self = Color::Idx(10);
  pub const BRIGHT_YELLOW: Self = Color::Idx(11);
  pub const BRIGHT_BLUE: Self = Color::Idx(12);
  pub const BRIGHT_MAGENTA: Self = Color::Idx(13);
  pub const BRIGHT_CYAN: Self = Color::Idx(14);
  pub const BRIGHT_WHITE: Self = Color::Idx(15);
}

impl Default for Color {
  fn default() -> Self {
    Self::Default
  }
}
