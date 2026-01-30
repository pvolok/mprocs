#![allow(clippy::as_conversions, clippy::pedantic)]

use compact_str::CompactString;
use unicode_width::UnicodeWidthStr;

/// Represents a single terminal cell.
#[derive(Clone, Debug, Default, Eq)]
pub struct Cell {
  text: CompactString,
  attrs: crate::vt100::attrs::Attrs,
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
  #[cfg(test)]
  pub fn new(content: &str) -> Cell {
    Cell {
      text: content.into(),
      attrs: Default::default(),
    }
  }

  #[cfg(test)]
  pub fn with_attrs(self, attrs: crate::vt100::attrs::Attrs) -> Cell {
    Cell { attrs, ..self }
  }

  pub(crate) fn set(&mut self, c: char, a: crate::vt100::attrs::Attrs) {
    self.text.clear();
    self.text.push(c);
    self.attrs = a;
  }

  pub(crate) fn set_str(&mut self, str: &str) {
    self.text.clear();
    self.text.push_str(str);
  }

  pub(crate) fn append(&mut self, c: char) {
    if self.text.is_empty() {
      self.text.push(' ');
    }
    self.text.push(c);
  }

  pub(crate) fn clear(&mut self, attrs: crate::vt100::attrs::Attrs) {
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

  pub fn width(&self) -> u16 {
    self.text.width() as u16
  }

  pub(crate) fn attrs(&self) -> &crate::vt100::attrs::Attrs {
    &self.attrs
  }

  pub(crate) fn set_attrs(&mut self, attrs: crate::vt100::attrs::Attrs) {
    self.attrs = attrs;
  }
}
