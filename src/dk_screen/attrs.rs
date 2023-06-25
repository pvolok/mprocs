use bitflags::bitflags;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attrs {
  pub fg: Color,
  pub bg: Color,
  pub mods: Mods,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Color {
  Default,
  Idx(u8),
  Rgb(u8, u8, u8),
}

impl Default for Color {
  fn default() -> Self {
    Self::Default
  }
}

impl Color {
  pub fn from_vt100(color: vt100::Color) -> Self {
    match color {
      vt100::Color::Default => Color::Default,
      vt100::Color::Idx(index) => Color::Idx(index),
      vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
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

bitflags! {
  #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
  pub struct Mods: u8 {
    const BOLD = 0b0000_0001;
    const ITALIC = 0b0000_0010;
    const UNDERLINE = 0b0000_0100;
    const INVERSE = 0b0000_1000;
  }
}

impl Mods {
  pub fn to_tui(self) -> tui::style::Modifier {
    let mut mods = tui::style::Modifier::empty();
    mods.set(tui::style::Modifier::BOLD, self.contains(Mods::BOLD));
    mods.set(tui::style::Modifier::ITALIC, self.contains(Mods::ITALIC));
    mods.set(tui::style::Modifier::REVERSED, self.contains(Mods::INVERSE));
    mods.set(
      tui::style::Modifier::UNDERLINED,
      self.contains(Mods::UNDERLINE),
    );
    mods
  }
}
