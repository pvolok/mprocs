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

  pub fn new_with_attrs(cols: u16, attrs: crate::vt100::attrs::Attrs) -> Self {
    let mut cell = crate::vt100::cell::Cell::default();
    cell.set_attrs(attrs);
    Self {
      cells: vec![cell; usize::from(cols)],
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
    vec.extend(self.cells.iter().take(self.size.into()).cloned());
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

        contents.push_str(cell.contents());
        prev_col += if cell.is_wide() { 2 } else { 1 };
      }
    }
    if prev_col == start && wrapping {
      contents.push('\n');
    }
  }

  pub(crate) fn is_wide_continuation(&self, col: u16) -> bool {
    if col == 0 {
      return false;
    }

    self
      .cells
      .get(Into::<usize>::into(col) - 1)
      .is_some_and(super::cell::Cell::is_wide)
  }
}
