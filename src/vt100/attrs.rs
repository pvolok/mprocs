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

const TEXT_MODE_BOLD: u8 = 0b0000_0001;
const TEXT_MODE_ITALIC: u8 = 0b0000_0010;
const TEXT_MODE_UNDERLINE: u8 = 0b0000_0100;
const TEXT_MODE_INVERSE: u8 = 0b0000_1000;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attrs {
  pub fgcolor: Color,
  pub bgcolor: Color,
  pub mode: u8,
}

impl Attrs {
  pub fn fg(&mut self, color: Color) -> Self {
    self.fgcolor = color;
    *self
  }

  pub fn bg(&mut self, color: Color) -> Self {
    self.bgcolor = color;
    *self
  }

  pub fn bold(&self) -> bool {
    self.mode & TEXT_MODE_BOLD != 0
  }

  pub fn set_bold(&mut self, bold: bool) -> Self {
    if bold {
      self.mode |= TEXT_MODE_BOLD;
    } else {
      self.mode &= !TEXT_MODE_BOLD;
    }
    *self
  }

  pub fn italic(&self) -> bool {
    self.mode & TEXT_MODE_ITALIC != 0
  }

  pub fn set_italic(&mut self, italic: bool) -> Self {
    if italic {
      self.mode |= TEXT_MODE_ITALIC;
    } else {
      self.mode &= !TEXT_MODE_ITALIC;
    }
    *self
  }

  pub fn underline(&self) -> bool {
    self.mode & TEXT_MODE_UNDERLINE != 0
  }

  pub fn set_underline(&mut self, underline: bool) -> Self {
    if underline {
      self.mode |= TEXT_MODE_UNDERLINE;
    } else {
      self.mode &= !TEXT_MODE_UNDERLINE;
    }
    *self
  }

  pub fn inverse(&self) -> bool {
    self.mode & TEXT_MODE_INVERSE != 0
  }

  pub fn set_inverse(&mut self, inverse: bool) -> Self {
    if inverse {
      self.mode |= TEXT_MODE_INVERSE;
    } else {
      self.mode &= !TEXT_MODE_INVERSE;
    }
    *self
  }
}
