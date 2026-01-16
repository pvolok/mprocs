use tui::style::Modifier;

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

impl Default for Color {
  fn default() -> Self {
    Self::Default
  }
}

impl Color {
  pub fn to_tui(self) -> tui::style::Color {
    match self {
      Color::Default => tui::style::Color::Reset,
      Color::Idx(index) => tui::style::Color::Indexed(index),
      Color::Rgb(r, g, b) => tui::style::Color::Rgb(r, g, b),
    }
  }
}

impl From<termwiz::color::ColorSpec> for Color {
  fn from(value: termwiz::color::ColorSpec) -> Self {
    match value {
      termwiz::color::ColorSpec::Default => Self::Default,
      termwiz::color::ColorSpec::PaletteIndex(idx) => Self::Idx(idx),
      termwiz::color::ColorSpec::TrueColor(srgba) => {
        let (r, g, b, _) = srgba.to_srgb_u8();
        Self::Rgb(r, g, b)
      }
    }
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
  pub fn bold(&self) -> bool {
    self.mode & TEXT_MODE_BOLD != 0
  }

  pub fn set_bold(&mut self, bold: bool) {
    if bold {
      self.mode |= TEXT_MODE_BOLD;
    } else {
      self.mode &= !TEXT_MODE_BOLD;
    }
  }

  pub fn italic(&self) -> bool {
    self.mode & TEXT_MODE_ITALIC != 0
  }

  pub fn set_italic(&mut self, italic: bool) {
    if italic {
      self.mode |= TEXT_MODE_ITALIC;
    } else {
      self.mode &= !TEXT_MODE_ITALIC;
    }
  }

  pub fn underline(&self) -> bool {
    self.mode & TEXT_MODE_UNDERLINE != 0
  }

  pub fn set_underline(&mut self, underline: bool) {
    if underline {
      self.mode |= TEXT_MODE_UNDERLINE;
    } else {
      self.mode &= !TEXT_MODE_UNDERLINE;
    }
  }

  pub fn inverse(&self) -> bool {
    self.mode & TEXT_MODE_INVERSE != 0
  }

  pub fn set_inverse(&mut self, inverse: bool) {
    if inverse {
      self.mode |= TEXT_MODE_INVERSE;
    } else {
      self.mode &= !TEXT_MODE_INVERSE;
    }
  }
}

impl Attrs {
  pub fn mods_to_tui(&self) -> tui::style::Modifier {
    let mut mods = Modifier::empty();
    mods.set(Modifier::BOLD, self.bold());
    mods.set(Modifier::ITALIC, self.italic());
    mods.set(Modifier::UNDERLINED, self.underline());
    mods.set(Modifier::REVERSED, self.inverse());
    mods
  }
}
