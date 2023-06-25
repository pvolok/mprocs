use compact_str::CompactString;

use super::attrs::{Attrs, Color, Mods};

pub struct Cell {
  text: CompactString,
  pub attrs: Attrs,
}

impl PartialEq<Self> for Cell {
  fn eq(&self, other: &Self) -> bool {
    if self.text != other.text {
      return false;
    }
    if self.attrs != other.attrs {
      return false;
    }
    true
  }
}

impl Cell {
  pub fn from_vt100(cell: &vt100::Cell) -> Self {
    let text = CompactString::from(cell.contents());

    let fg = Color::from_vt100(cell.fgcolor());
    let bg = Color::from_vt100(cell.bgcolor());

    let mut mods = Mods::empty();
    mods.set(Mods::BOLD, cell.bold());
    mods.set(Mods::ITALIC, cell.italic());
    mods.set(Mods::INVERSE, cell.inverse());
    mods.set(Mods::UNDERLINE, cell.underline());

    let attrs = Attrs { fg, bg, mods };

    Self { text, attrs }
  }
}

impl Cell {
  pub fn to_tui(self) -> tui::buffer::Cell {
    tui::buffer::Cell {
      symbol: self.text.into_string(),
      fg: self.attrs.fg.to_tui(),
      bg: self.attrs.bg.to_tui(),
      modifier: self.attrs.mods.to_tui(),
    }
  }
}
