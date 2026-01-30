use std::fmt::Write;

use unicode_width::UnicodeWidthStr;

use crate::{
  protocol::CursorStyle,
  vt100::{
    attrs::Attrs,
    grid::{Grid, Pos},
    Cell, Size,
  },
};

pub struct ScreenDiffer {
  cells: Vec<Cell>,
  brush: Attrs,
  pos: Pos,
  cursor_pos: Option<Pos>,
  cursor_style: CursorStyle,
}

pub trait BufferView {
  fn size(&self) -> Size;
  fn get_cell(&self, pos: Pos) -> Option<&Cell>;
  fn get_cursor_pos(&self) -> Option<Pos>;
  fn get_cursor_style(&self) -> CursorStyle;
}

impl ScreenDiffer {
  pub fn new() -> Self {
    Self {
      cells: Vec::new(),
      brush: Attrs::default(),
      pos: Pos { row: 0, col: 0 },
      cursor_pos: Some(Pos { col: 0, row: 0 }),
      cursor_style: CursorStyle::default(),
    }
  }

  pub fn diff<V: BufferView, W: Write>(
    &mut self,
    w: &mut W,
    view: &V,
  ) -> std::fmt::Result {
    let prev = &mut self.cells;
    let brush = &mut self.brush;

    let size = view.size();
    let mut full_rerender = false;
    if (size.height * size.width) as usize != prev.len() {
      full_rerender = true;
      prev.resize((size.height * size.width) as usize, Cell::default());
    }
    for y in 0..size.height {
      for x in 0..size.width {
        let offset = (size.width * y + x) as usize;
        let cell = view
          .get_cell(Pos { col: x, row: y })
          .cloned()
          .unwrap_or_default();

        let mut sep = {
          let mut first = true;
          move |w: &mut W| {
            if first {
              first = false;
              Ok(())
            } else {
              write!(w, ";")
            }
          }
        };

        if full_rerender || cell != prev[offset] {
          let attrs = *cell.attrs();

          if *brush != attrs {
            write!(w, "\x1b[")?;
            if brush.fgcolor != attrs.fgcolor {
              sep(w)?;
              match attrs.fgcolor {
                super::Color::Default => write!(w, "39")?,
                super::Color::Idx(idx) => write!(w, "38;5;{}", idx)?,
                super::Color::Rgb(r, g, b) => write!(w, "38;2;{r};{g};{b}")?,
              }
            }
            if brush.bgcolor != attrs.bgcolor {
              sep(w)?;
              match attrs.bgcolor {
                super::Color::Default => write!(w, "49")?,
                super::Color::Idx(idx) => write!(w, "48;5;{}", idx)?,
                super::Color::Rgb(r, g, b) => write!(w, "48;2;{r};{g};{b}")?,
              }
            }
            if brush.bold() != attrs.bold() {
              sep(w)?;
              let value = if attrs.bold() { 1 } else { 22 };
              write!(w, "{value}")?;
            }
            if brush.italic() != attrs.italic() {
              sep(w)?;
              let value = if attrs.italic() { 3 } else { 23 };
              write!(w, "{value}")?;
            }
            if brush.underline() != attrs.underline() {
              sep(w)?;
              let value = if attrs.italic() { 4 } else { 24 };
              write!(w, "{value}")?;
            }
            if brush.inverse() != attrs.inverse() {
              sep(w)?;
              let value = if attrs.inverse() { 7 } else { 27 };
              write!(w, "{value}")?;
            }
            write!(w, "m")?;

            *brush = attrs;
          }

          let pos = Pos { row: y, col: x };
          if self.pos != pos {
            write!(w, "\x1b[{};{}H", pos.row + 1, pos.col + 1)?;
            self.pos = pos;
          }

          let c = if cell.width() > 0 {
            cell.contents()
          } else {
            " "
          };
          write!(w, "{}", c)?;
          self.pos.col = (size.width - 1).min(self.pos.col + c.width() as u16);
          prev[offset] = cell;
        }
      }
    }

    if self.cursor_pos.is_some() != view.get_cursor_pos().is_some() {
      if view.get_cursor_pos().is_some() {
        write!(w, "\x1b[?25h")?;
      } else {
        write!(w, "\x1b[?25l")?;
      }
    }
    if let Some(pos) = view.get_cursor_pos() {
      if self.pos != pos {
        write!(w, "\x1b[{};{}H", pos.row + 1, pos.col + 1)?;
      }
      self.pos = pos;
    }
    self.cursor_pos = view.get_cursor_pos();

    if self.cursor_style != view.get_cursor_style() {
      let cmd = match view.get_cursor_style() {
        CursorStyle::Default => 0,
        CursorStyle::BlinkingBlock => 1,
        CursorStyle::SteadyBlock => 2,
        CursorStyle::BlinkingUnderline => 3,
        CursorStyle::SteadyUnderline => 4,
        CursorStyle::BlinkingBar => 5,
        CursorStyle::SteadyBar => 6,
      };
      write!(w, "\x1b[{} q", cmd)?;
      self.cursor_style = view.get_cursor_style();
    }

    Ok(())
  }
}

impl BufferView for Vec<Vec<Cell>> {
  fn size(&self) -> Size {
    Size {
      height: self.len() as u16,
      width: self.get(0).map_or(0, |row| row.len() as u16),
    }
  }

  fn get_cell(&self, pos: Pos) -> Option<&Cell> {
    self
      .get(pos.row as usize)
      .map(|row| row.get(pos.col as usize))
      .flatten()
  }

  fn get_cursor_pos(&self) -> Option<Pos> {
    None
  }

  fn get_cursor_style(&self) -> CursorStyle {
    CursorStyle::Default
  }
}

impl BufferView for Grid {
  fn size(&self) -> Size {
    self.size()
  }

  fn get_cell(&self, pos: Pos) -> Option<&Cell> {
    self.visible_cell(pos)
  }

  fn get_cursor_pos(&self) -> Option<Pos> {
    self.cursor_pos
  }

  fn get_cursor_style(&self) -> CursorStyle {
    self.cursor_style
  }
}

#[cfg(test)]
mod tests {
  use crate::vt100::Color;

  use super::*;

  #[test]
  fn basic() {
    let attrs = Attrs {
      fgcolor: Color::Idx(4),
      ..Default::default()
    };

    let mut differ = ScreenDiffer::new();
    let mut out = String::new();

    let screen = vec![vec![
      Cell::new("1"),
      Cell::new("2"),
      Cell::new("3").with_attrs(attrs),
      Cell::new("4").with_attrs(attrs),
      Cell::new("5"),
    ]];
    differ.diff(&mut out, &screen).unwrap();
    assert_eq!("12\x1b[38;5;4m34\x1b[39m5", out);

    let screen = vec![vec![
      Cell::new("1"),
      Cell::new("_"),
      Cell::new("3"),
      Cell::new("4").with_attrs(attrs),
      Cell::new("5"),
    ]];
    out.clear();
    differ.diff(&mut out, &screen).unwrap();
    assert_eq!("\x1b[1;2H_3", out);
  }
}
