use crate::vt100::term::BufWrite as _;

use super::Cell;

#[derive(Clone, Debug)]
pub struct Row {
  pub cells: Vec<crate::vt100::cell::Cell>,
  size: u16,
  wrapped: bool,
}

impl Row {
  pub fn new(cols: u16) -> Self {
    Self {
      cells: vec![crate::vt100::cell::Cell::default(); usize::from(cols)],
      size: 0,
      wrapped: false,
    }
  }

  pub fn cols(&self) -> u16 {
    self
      .cells
      .len()
      .try_into()
      // we limit the number of cols to a u16 (see Size)
      .unwrap()
  }

  pub fn clear(&mut self, attrs: crate::vt100::attrs::Attrs) {
    for cell in &mut self.cells {
      cell.clear(attrs);
    }
    self.size = 0;
    self.wrapped = false;
  }

  fn cells(&self) -> impl Iterator<Item = &crate::vt100::cell::Cell> {
    self.cells.iter()
  }

  pub fn get(&self, col: u16) -> Option<&crate::vt100::cell::Cell> {
    self.cells.get(usize::from(col))
  }

  pub fn get_mut(&mut self, col: u16) -> Option<&mut crate::vt100::cell::Cell> {
    self.size = self.size.max(col + 1);
    self.cells.get_mut(usize::from(col))
  }

  pub fn insert(&mut self, i: u16, cell: crate::vt100::cell::Cell) {
    self.cells.insert(usize::from(i), cell);
    self.wrapped = false;
  }

  pub fn remove(&mut self, i: u16) {
    self.clear_wide(i);
    self.cells.remove(usize::from(i));
    self.wrapped = false;
  }

  pub fn erase(&mut self, i: u16, attrs: crate::vt100::attrs::Attrs) {
    let wide = self.cells[usize::from(i)].is_wide();
    self.clear_wide(i);
    self.cells[usize::from(i)].clear(attrs);
    if i == self.cols() - if wide { 2 } else { 1 } {
      self.wrapped = false;
    }
  }

  pub fn truncate(&mut self, len: u16) {
    self.cells.truncate(usize::from(len));
    self.wrapped = false;
    let last_cell = &mut self.cells[usize::from(len) - 1];
    if last_cell.is_wide() {
      last_cell.clear(*last_cell.attrs());
    }
  }

  pub fn resize(&mut self, len: u16, cell: crate::vt100::cell::Cell) {
    self.cells.resize(usize::from(len), cell);
    self.wrapped = false;
  }

  pub fn wrap(&mut self, wrap: bool) {
    self.wrapped = wrap;
  }

  pub fn wrapped(&self) -> bool {
    self.wrapped
  }

  pub fn clear_wide(&mut self, col: u16) {
    let cell = &self.cells[usize::from(col)];
    let other = if cell.is_wide() {
      self.cells.get_mut(usize::from(col + 1))
    } else if self.is_wide_continuation(col) {
      self.cells.get_mut(usize::from(col - 1))
    } else {
      return;
    };
    if let Some(other) = other {
      other.clear(*other.attrs());
    }
  }

  pub fn take_cells(&self, vec: &mut Vec<Cell>) {
    vec.extend(self.cells.iter().take(self.size as _).cloned());
  }

  pub fn write_contents(
    &self,
    contents: &mut String,
    start: u16,
    width: u16,
    wrapping: bool,
  ) {
    let mut prev_was_wide = false;

    let mut prev_col = start;
    for (col, cell) in self
      .cells()
      .enumerate()
      .skip(usize::from(start))
      .take(usize::from(width))
    {
      if prev_was_wide {
        prev_was_wide = false;
        continue;
      }
      prev_was_wide = cell.is_wide();

      // we limit the number of cols to a u16 (see Size)
      let col: u16 = col.try_into().unwrap();
      if cell.has_contents() {
        for _ in 0..(col - prev_col) {
          contents.push(' ');
        }
        prev_col += col - prev_col;

        contents.push_str(&cell.contents());
        prev_col += if cell.is_wide() { 2 } else { 1 };
      }
    }
    if prev_col == start && wrapping {
      contents.push('\n');
    }
  }

  pub fn write_contents_formatted(
    &self,
    contents: &mut Vec<u8>,
    start: u16,
    width: u16,
    row: u16,
    wrapping: bool,
    prev_pos: Option<crate::vt100::grid::Pos>,
    prev_attrs: Option<crate::vt100::attrs::Attrs>,
  ) -> (crate::vt100::grid::Pos, crate::vt100::attrs::Attrs) {
    let mut prev_was_wide = false;
    let default_cell = crate::vt100::cell::Cell::default();

    let mut prev_pos = if let Some(prev_pos) = prev_pos {
      prev_pos
    } else if wrapping {
      crate::vt100::grid::Pos {
        row: row - 1,
        col: self.cols(),
      }
    } else {
      crate::vt100::grid::Pos { row, col: start }
    };
    let mut prev_attrs = prev_attrs.unwrap_or_default();

    let first_cell = &self.cells[usize::from(start)];
    if wrapping && first_cell == &default_cell {
      let default_attrs = default_cell.attrs();
      if &prev_attrs != default_attrs {
        default_attrs.write_escape_code_diff(contents, &prev_attrs);
        prev_attrs = *default_attrs;
      }
      contents.push(b' ');
      crate::vt100::term::Backspace::default().write_buf(contents);
      crate::vt100::term::EraseChar::new(1).write_buf(contents);
      prev_pos = crate::vt100::grid::Pos { row, col: 0 };
    }

    let mut erase: Option<(u16, &crate::vt100::attrs::Attrs)> = None;
    for (col, cell) in self
      .cells()
      .enumerate()
      .skip(usize::from(start))
      .take(usize::from(width))
    {
      if prev_was_wide {
        prev_was_wide = false;
        continue;
      }
      prev_was_wide = cell.is_wide();

      // we limit the number of cols to a u16 (see Size)
      let col: u16 = col.try_into().unwrap();
      let pos = crate::vt100::grid::Pos { row, col };

      if let Some((prev_col, attrs)) = erase {
        if cell.has_contents() || cell.attrs() != attrs {
          let new_pos = crate::vt100::grid::Pos { row, col: prev_col };
          if wrapping
            && prev_pos.row + 1 == new_pos.row
            && prev_pos.col >= self.cols()
          {
            if new_pos.col > 0 {
              contents.extend(" ".repeat(usize::from(new_pos.col)).as_bytes());
            } else {
              contents.extend(b" ");
              crate::vt100::term::Backspace::default().write_buf(contents);
            }
          } else {
            crate::vt100::term::MoveFromTo::new(prev_pos, new_pos)
              .write_buf(contents);
          }
          prev_pos = new_pos;
          if &prev_attrs != attrs {
            attrs.write_escape_code_diff(contents, &prev_attrs);
            prev_attrs = *attrs;
          }
          crate::vt100::term::EraseChar::new(pos.col - prev_col)
            .write_buf(contents);
          erase = None;
        }
      }

      if cell != &default_cell {
        let attrs = cell.attrs();
        if cell.has_contents() {
          if pos != prev_pos {
            if !wrapping
              || prev_pos.row + 1 != pos.row
              || prev_pos.col < self.cols() - if cell.is_wide() { 1 } else { 0 }
              || pos.col != 0
            {
              crate::vt100::term::MoveFromTo::new(prev_pos, pos)
                .write_buf(contents);
            }
            prev_pos = pos;
          }

          if &prev_attrs != attrs {
            attrs.write_escape_code_diff(contents, &prev_attrs);
            prev_attrs = *attrs;
          }

          prev_pos.col += if cell.is_wide() { 2 } else { 1 };
          let cell_contents = cell.contents();
          contents.extend(cell_contents.as_bytes());
        } else if erase.is_none() {
          erase = Some((pos.col, attrs));
        }
      }
    }
    if let Some((prev_col, attrs)) = erase {
      let new_pos = crate::vt100::grid::Pos { row, col: prev_col };
      if wrapping
        && prev_pos.row + 1 == new_pos.row
        && prev_pos.col >= self.cols()
      {
        if new_pos.col > 0 {
          contents.extend(" ".repeat(usize::from(new_pos.col)).as_bytes());
        } else {
          contents.extend(b" ");
          crate::vt100::term::Backspace::default().write_buf(contents);
        }
      } else {
        crate::vt100::term::MoveFromTo::new(prev_pos, new_pos)
          .write_buf(contents);
      }
      prev_pos = new_pos;
      if &prev_attrs != attrs {
        attrs.write_escape_code_diff(contents, &prev_attrs);
        prev_attrs = *attrs;
      }
      crate::vt100::term::ClearRowForward::default().write_buf(contents);
    }

    (prev_pos, prev_attrs)
  }

  // while it's true that most of the logic in this is identical to
  // write_contents_formatted, i can't figure out how to break out the
  // common parts without making things noticeably slower.
  pub fn write_contents_diff(
    &self,
    contents: &mut Vec<u8>,
    prev: &Self,
    start: u16,
    width: u16,
    row: u16,
    wrapping: bool,
    prev_wrapping: bool,
    mut prev_pos: crate::vt100::grid::Pos,
    mut prev_attrs: crate::vt100::attrs::Attrs,
  ) -> (crate::vt100::grid::Pos, crate::vt100::attrs::Attrs) {
    let mut prev_was_wide = false;

    let first_cell = &self.cells[usize::from(start)];
    let prev_first_cell = &prev.cells[usize::from(start)];
    if wrapping
      && !prev_wrapping
      && first_cell == prev_first_cell
      && prev_pos.row + 1 == row
      && prev_pos.col
        >= self.cols() - if prev_first_cell.is_wide() { 1 } else { 0 }
    {
      let first_cell_attrs = first_cell.attrs();
      if &prev_attrs != first_cell_attrs {
        first_cell_attrs.write_escape_code_diff(contents, &prev_attrs);
        prev_attrs = *first_cell_attrs;
      }
      let mut cell_contents = prev_first_cell.contents();
      let need_erase = if cell_contents.is_empty() {
        cell_contents = " ";
        true
      } else {
        false
      };
      contents.extend(cell_contents.as_bytes());
      crate::vt100::term::Backspace::default().write_buf(contents);
      if prev_first_cell.is_wide() {
        crate::vt100::term::Backspace::default().write_buf(contents);
      }
      if need_erase {
        crate::vt100::term::EraseChar::new(1).write_buf(contents);
      }
      prev_pos = crate::vt100::grid::Pos { row, col: 0 };
    }

    let mut erase: Option<(u16, &crate::vt100::attrs::Attrs)> = None;
    for (col, (cell, prev_cell)) in self
      .cells()
      .zip(prev.cells())
      .enumerate()
      .skip(usize::from(start))
      .take(usize::from(width))
    {
      if prev_was_wide {
        prev_was_wide = false;
        continue;
      }
      prev_was_wide = cell.is_wide();

      // we limit the number of cols to a u16 (see Size)
      let col: u16 = col.try_into().unwrap();
      let pos = crate::vt100::grid::Pos { row, col };

      if let Some((prev_col, attrs)) = erase {
        if cell.has_contents() || cell.attrs() != attrs {
          let new_pos = crate::vt100::grid::Pos { row, col: prev_col };
          if wrapping
            && prev_pos.row + 1 == new_pos.row
            && prev_pos.col >= self.cols()
          {
            if new_pos.col > 0 {
              contents.extend(" ".repeat(usize::from(new_pos.col)).as_bytes());
            } else {
              contents.extend(b" ");
              crate::vt100::term::Backspace::default().write_buf(contents);
            }
          } else {
            crate::vt100::term::MoveFromTo::new(prev_pos, new_pos)
              .write_buf(contents);
          }
          prev_pos = new_pos;
          if &prev_attrs != attrs {
            attrs.write_escape_code_diff(contents, &prev_attrs);
            prev_attrs = *attrs;
          }
          crate::vt100::term::EraseChar::new(pos.col - prev_col)
            .write_buf(contents);
          erase = None;
        }
      }

      if cell != prev_cell {
        let attrs = cell.attrs();
        if cell.has_contents() {
          if pos != prev_pos {
            if !wrapping
              || prev_pos.row + 1 != pos.row
              || prev_pos.col < self.cols() - if cell.is_wide() { 1 } else { 0 }
              || pos.col != 0
            {
              crate::vt100::term::MoveFromTo::new(prev_pos, pos)
                .write_buf(contents);
            }
            prev_pos = pos;
          }

          if &prev_attrs != attrs {
            attrs.write_escape_code_diff(contents, &prev_attrs);
            prev_attrs = *attrs;
          }

          prev_pos.col += if cell.is_wide() { 2 } else { 1 };
          contents.extend(cell.contents().as_bytes());
        } else if erase.is_none() {
          erase = Some((pos.col, attrs));
        }
      }
    }
    if let Some((prev_col, attrs)) = erase {
      let new_pos = crate::vt100::grid::Pos { row, col: prev_col };
      if wrapping
        && prev_pos.row + 1 == new_pos.row
        && prev_pos.col >= self.cols()
      {
        if new_pos.col > 0 {
          contents.extend(" ".repeat(usize::from(new_pos.col)).as_bytes());
        } else {
          contents.extend(b" ");
          crate::vt100::term::Backspace::default().write_buf(contents);
        }
      } else {
        crate::vt100::term::MoveFromTo::new(prev_pos, new_pos)
          .write_buf(contents);
      }
      prev_pos = new_pos;
      if &prev_attrs != attrs {
        attrs.write_escape_code_diff(contents, &prev_attrs);
        prev_attrs = *attrs;
      }
      crate::vt100::term::ClearRowForward::default().write_buf(contents);
    }

    // if this row is going from wrapped to not wrapped, we need to erase
    // and redraw the last character to break wrapping. if this row is
    // wrapped, we need to redraw the last character without erasing it to
    // position the cursor after the end of the line correctly so that
    // drawing the next line can just start writing and be wrapped.
    if (!self.wrapped && prev.wrapped) || (!prev.wrapped && self.wrapped) {
      let end_pos = if self.is_wide_continuation(self.cols() - 1) {
        crate::vt100::grid::Pos {
          row,
          col: self.cols() - 2,
        }
      } else {
        crate::vt100::grid::Pos {
          row,
          col: self.cols() - 1,
        }
      };
      crate::vt100::term::MoveFromTo::new(prev_pos, end_pos)
        .write_buf(contents);
      prev_pos = end_pos;
      if !self.wrapped {
        crate::vt100::term::EraseChar::new(1).write_buf(contents);
      }
      let end_cell = &self.cells[usize::from(end_pos.col)];
      if end_cell.has_contents() {
        let attrs = end_cell.attrs();
        if &prev_attrs != attrs {
          attrs.write_escape_code_diff(contents, &prev_attrs);
          prev_attrs = *attrs;
        }
        contents.extend(end_cell.contents().as_bytes());
        prev_pos.col += if end_cell.is_wide() { 2 } else { 1 };
      }
    }

    (prev_pos, prev_attrs)
  }

  pub(crate) fn is_wide_continuation(&self, col: u16) -> bool {
    if col == 0 {
      return false;
    }

    self
      .cells
      .get(col as usize - 1)
      .map_or(false, |c| c.is_wide())
  }
}
