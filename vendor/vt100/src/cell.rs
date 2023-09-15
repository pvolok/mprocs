use compact_str::CompactString;
use unicode_width::UnicodeWidthStr;

/// Represents a single terminal cell.
#[derive(Clone, Debug, Default, Eq)]
pub struct Cell {
  text: CompactString,
  attrs: crate::attrs::Attrs,
}

impl PartialEq<Self> for Cell {
  fn eq(&self, other: &Self) -> bool {
    if self.text != other.text {
      return false;
    }
    if self.attrs != other.attrs {
      return false;
    }
    return true;
  }
}

impl Cell {
  pub(crate) fn set(&mut self, c: char, a: crate::attrs::Attrs) {
    self.text.clear();
    self.text.push(c);
    self.attrs = a;
  }

  pub(crate) fn append(&mut self, c: char) {
    if self.text.is_empty() {
      self.text.push(' ');
    }
    self.text.push(c);
  }

  pub(crate) fn clear(&mut self, attrs: crate::attrs::Attrs) {
    self.text.clear();
    self.attrs = attrs;
  }

  /// Returns the text contents of the cell.
  ///
  /// Can include multiple unicode characters if combining characters are
  /// used, but will contain at most one character with a non-zero character
  /// width.
  #[must_use]
  pub fn contents(&self) -> &str {
    self.text.as_str()
  }

  /// Returns whether the cell contains any text data.
  #[must_use]
  pub fn has_contents(&self) -> bool {
    !self.text.is_empty()
  }

  /// Returns whether the text data in the cell represents a wide character.
  #[must_use]
  pub fn is_wide(&self) -> bool {
    self.text.width() >= 2
  }

  pub(crate) fn attrs(&self) -> &crate::attrs::Attrs {
    &self.attrs
  }

  /// Returns the foreground color of the cell.
  #[must_use]
  pub fn fgcolor(&self) -> crate::attrs::Color {
    self.attrs.fgcolor
  }

  /// Returns the background color of the cell.
  #[must_use]
  pub fn bgcolor(&self) -> crate::attrs::Color {
    self.attrs.bgcolor
  }

  /// Returns whether the cell should be rendered with the bold text
  /// attribute.
  #[must_use]
  pub fn bold(&self) -> bool {
    self.attrs.bold()
  }

  /// Returns whether the cell should be rendered with the italic text
  /// attribute.
  #[must_use]
  pub fn italic(&self) -> bool {
    self.attrs.italic()
  }

  /// Returns whether the cell should be rendered with the underlined text
  /// attribute.
  #[must_use]
  pub fn underline(&self) -> bool {
    self.attrs.underline()
  }

  /// Returns whether the cell should be rendered with the inverse text
  /// attribute.
  #[must_use]
  pub fn inverse(&self) -> bool {
    self.attrs.inverse()
  }
}

impl Cell {
  pub fn to_tui(&self) -> tui::buffer::Cell {
    let fg = self.attrs.fgcolor.to_tui();
    tui::buffer::Cell {
      symbol: self.text.to_string(),
      fg,
      bg: self.attrs.bgcolor.to_tui(),
      modifier: self.attrs.mods_to_tui(),
      underline_color: fg,
      skip: false,
    }
  }
}
