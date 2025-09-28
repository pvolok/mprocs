use std::collections::VecDeque;

use super::{attrs::Attrs, row::Row, Cell};

#[derive(Clone, Debug)]
pub struct Grid {
  size: Size,
  pos: Pos,
  saved_pos: Pos,
  scroll_top: u16,
  scroll_bottom: u16,
  rows: VecDeque<crate::vt100::row::Row>,
  /// Number of visible rows that were printed to. On resizing unused rows can
  /// be removed.
  used_rows: u16,
  origin_mode: bool,
  saved_origin_mode: bool,
  scrollback_len: usize,
  scrollback_offset: usize,
}

impl Grid {
  pub fn new(size: Size, scrollback_len: usize) -> Self {
    let mut rows = VecDeque::with_capacity(size.rows.into());
    for _ in 0..size.rows {
      rows.push_back(Row::new(size.cols));
    }

    Self {
      size,
      pos: Pos::default(),
      saved_pos: Pos::default(),
      scroll_top: 0,
      scroll_bottom: size.rows - 1,
      rows,
      used_rows: 0,
      origin_mode: false,
      saved_origin_mode: false,
      scrollback_len,
      scrollback_offset: 0,
    }
  }

  pub fn get_selected_text(
    &self,
    low_x: i32,
    low_y: i32,
    high_x: i32,
    high_y: i32,
  ) -> String {
    let lines = self
      .rows
      .iter()
      .skip((self.row0() as i32 + low_y) as usize)
      .take((high_y - low_y) as usize + 1)
      .enumerate();

    let mut contents = String::new();

    let lines_len = high_y - low_y + 1;
    for (i, row) in lines {
      let i = i as i32;
      let start = if i == 0 { low_x } else { 0 };

      let width = row.cols() as i32;
      let width = if i == lines_len - 1 {
        width.min(high_x + 1)
      } else {
        width
      };
      let width = width - start;

      row.write_contents(&mut contents, start as u16, width as u16, false);
      if i != lines_len - 1 && !row.wrapped() {
        contents.push('\n');
      }
    }

    contents
  }

  fn new_row(&self) -> Row {
    Row::new(self.size.cols)
  }

  pub fn clear(&mut self) {
    self.pos = Pos::default();
    self.saved_pos = Pos::default();
    for row in self.drawing_rows_mut() {
      row.clear(Attrs::default());
    }
    self.scroll_top = 0;
    self.scroll_bottom = self.size.rows - 1;
    self.used_rows = 0;
    self.origin_mode = false;
    self.saved_origin_mode = false;
  }

  pub fn size(&self) -> Size {
    self.size
  }

  pub fn set_size(&mut self, size: Size) {
    let mut acc = VecDeque::with_capacity(self.rows.capacity());

    let prev_abs_pos_row = self.row0() + self.pos.row as usize;
    let mut abs_pos_row = 0;
    let max_rows =
      self.rows.len() - self.size.rows as usize + self.used_rows as usize;
    let mut rows = self.rows.drain(..).enumerate();

    let mut line = Vec::new();
    'rows: while let Some((i, mut row)) = rows.next() {
      if i >= max_rows {
        break;
      }
      line.clear();

      if prev_abs_pos_row == i {
        abs_pos_row = acc.len();
      }
      row.take_cells(&mut line);

      while row.wrapped() {
        if let Some((i, next_row)) = rows.next() {
          if i >= max_rows {
            break 'rows;
          }

          if prev_abs_pos_row == i {
            abs_pos_row = acc.len();
          }
          next_row.take_cells(&mut line);
          row = next_row;
        } else {
          break;
        }
      }

      let mut i = 0;
      loop {
        let mut new_row = Row::new(size.cols);

        let mut j = 0;
        while line.len() > i && j + line[i].width() <= size.cols {
          if let Some(target) = new_row.get_mut(j) {
            *target = line[i].clone();
          }

          j += 1;
          i += 1;
        }

        if line.len() > i {
          new_row.wrap(true);
          acc.push_back(new_row);
        } else {
          acc.push_back(new_row);
          break;
        }
      }
    }
    drop(rows);
    self.rows = acc;

    if self.scroll_bottom == self.size.rows - 1 {
      self.scroll_bottom = size.rows - 1;
    }

    self.size = size;

    if self.scroll_bottom >= size.rows {
      self.scroll_bottom = size.rows - 1;
    }
    if self.scroll_bottom < self.scroll_top {
      self.scroll_top = 0;
    }

    self.used_rows = u16::try_from(self.rows.len())
      .unwrap_or_default()
      .min(self.size.rows);
    while self.rows.len() < self.size.rows.into() {
      self.rows.push_back(self.new_row());
    }
    self.pos.row = u16::try_from(abs_pos_row.saturating_sub(self.row0()))
      .unwrap_or_default();

    self.row_clamp_top(false);
    self.row_clamp_bottom(false);
    self.col_clamp();

    while self.rows.len() < self.size.rows.into() {
      self.rows.push_back(self.new_row());
    }
  }

  pub fn pos(&self) -> Pos {
    self.pos
  }

  pub fn set_pos(&mut self, mut pos: Pos) {
    if self.origin_mode {
      pos.row = pos.row.saturating_add(self.scroll_top);
    }
    self.pos = pos;
    self.row_clamp_top(self.origin_mode);
    self.row_clamp_bottom(self.origin_mode);
    self.col_clamp();
  }

  pub fn save_cursor(&mut self) {
    self.saved_pos = self.pos;
    self.saved_origin_mode = self.origin_mode;
  }

  pub fn restore_cursor(&mut self) {
    self.pos = self.saved_pos;
    self.origin_mode = self.saved_origin_mode;
  }

  fn row0(&self) -> usize {
    self.rows.len() - self.size.rows as usize
  }

  pub fn visible_rows(&self) -> impl Iterator<Item = &Row> {
    self.rows.iter().skip(self.row0() - self.scrollback_offset)
  }

  pub fn drawing_rows(&self) -> impl Iterator<Item = &Row> {
    self.rows.iter().skip(self.row0())
  }

  pub fn drawing_rows_mut(&mut self) -> impl Iterator<Item = &mut Row> {
    let row0 = self.row0();
    self.rows.iter_mut().skip(row0)
  }

  pub fn visible_row(&self, row: u16) -> Option<&Row> {
    self.visible_rows().nth(usize::from(row))
  }

  pub fn drawing_row(&self, row: u16) -> Option<&Row> {
    self.drawing_rows().nth(usize::from(row))
  }

  pub fn drawing_row_mut(&mut self, row: u16) -> Option<&mut Row> {
    self.drawing_rows_mut().nth(usize::from(row))
  }

  pub fn current_row_mut(&mut self) -> &mut Row {
    self
      .drawing_row_mut(self.pos.row)
      // we assume self.pos.row is always valid
      .unwrap()
  }

  pub fn visible_cell(&self, pos: Pos) -> Option<&Cell> {
    self.visible_row(pos.row).and_then(|r| r.get(pos.col))
  }

  pub fn drawing_cell(&self, pos: Pos) -> Option<&Cell> {
    self.drawing_row(pos.row).and_then(|r| r.get(pos.col))
  }

  pub fn drawing_cell_mut(&mut self, pos: Pos) -> Option<&mut Cell> {
    self.used_rows = self.used_rows.max(pos.row + 1);
    self
      .drawing_row_mut(pos.row)
      .and_then(|r| r.get_mut(pos.col))
  }

  pub fn scrollback_len(&self) -> usize {
    self.scrollback_len
  }

  pub fn scrollback(&self) -> usize {
    self.scrollback_offset
  }

  pub fn set_scrollback(&mut self, rows: usize) {
    self.scrollback_offset = rows.min(self.row0());
  }

  pub fn erase_all(&mut self, attrs: Attrs) {
    self.used_rows = 0;
    for row in self.drawing_rows_mut() {
      row.clear(attrs);
    }
  }

  pub fn erase_all_forward(&mut self, attrs: Attrs) {
    self.used_rows = self.used_rows.min(self.pos.row + 1);
    let pos = self.pos;
    for row in self.drawing_rows_mut().skip(usize::from(pos.row) + 1) {
      row.clear(attrs);
    }

    self.erase_row_forward(attrs);
  }

  pub fn erase_all_backward(&mut self, attrs: Attrs) {
    let pos = self.pos;
    for row in self.drawing_rows_mut().take(usize::from(pos.row)) {
      row.clear(attrs);
    }

    self.erase_row_backward(attrs);
  }

  pub fn erase_row(&mut self, attrs: Attrs) {
    self.current_row_mut().clear(attrs);
  }

  pub fn erase_row_forward(&mut self, attrs: Attrs) {
    let size = self.size;
    let pos = self.pos;
    let row = self.current_row_mut();
    for col in pos.col..size.cols {
      row.erase(col, attrs);
    }
  }

  pub fn erase_row_backward(&mut self, attrs: Attrs) {
    let size = self.size;
    let pos = self.pos;
    let row = self.current_row_mut();
    for col in 0..=pos.col.min(size.cols - 1) {
      row.erase(col, attrs);
    }
  }

  pub fn insert_cells(&mut self, count: u16) {
    let size = self.size;
    let pos = self.pos;
    let row = self.current_row_mut();
    for _ in 0..count {
      row.insert(pos.col, Cell::default());
    }
    row.truncate(size.cols);
  }

  pub fn delete_cells(&mut self, count: u16) {
    let size = self.size;
    let pos = self.pos;
    let row = self.current_row_mut();
    for _ in 0..(count.min(size.cols - pos.col)) {
      row.remove(pos.col);
    }
    row.resize(size.cols, Cell::default());
  }

  pub fn erase_cells(&mut self, count: u16, attrs: Attrs) {
    let size = self.size;
    let pos = self.pos;
    let row = self.current_row_mut();
    for col in pos.col..((pos.col.saturating_add(count)).min(size.cols)) {
      row.erase(col, attrs);
    }
  }

  pub fn insert_lines(&mut self, count: u16) {
    let row0 = self.row0();
    for _ in 0..count {
      self.rows.remove(row0 + usize::from(self.scroll_bottom));
      self
        .rows
        .insert(row0 + usize::from(self.pos.row), self.new_row());
      // self.scroll_bottom is maintained to always be a valid row
      self.rows[row0 + usize::from(self.scroll_bottom)].wrap(false);
    }
  }

  pub fn delete_lines(&mut self, count: u16) {
    let row0 = self.row0();
    for _ in 0..(count.min(self.size.rows - self.pos.row)) {
      self
        .rows
        .insert(row0 + usize::from(self.scroll_bottom) + 1, self.new_row());
      self.rows.remove(row0 + usize::from(self.pos.row));
    }
  }

  pub fn scroll_up(&mut self, count: u16) {
    for _ in 0..(count.min(self.size.rows - self.scroll_top)) {
      let row0 = self.row0();
      self
        .rows
        .insert(row0 + usize::from(self.scroll_bottom) + 1, self.new_row());
      let removed = self.rows.remove(row0 + usize::from(self.scroll_top));
      if self.scrollback_len > 0 && !self.scroll_region_active() {
        if let Some(removed) = removed {
          self.rows.insert(row0, removed);
        }
        while self.rows.len() - self.size.rows as usize > self.scrollback_len {
          self.rows.pop_front();
        }
        if self.scrollback_offset > 0 {
          self.scrollback_offset = (self.rows.len() - self.size.rows as usize)
            .min(self.scrollback_offset + 1);
        }
      }
    }
  }

  pub fn scroll_down(&mut self, count: u16) {
    for _ in 0..count {
      let row0 = self.row0();
      self.rows.remove(row0 + usize::from(self.scroll_bottom));
      self
        .rows
        .insert(row0 + usize::from(self.scroll_top), self.new_row());
      // self.scroll_bottom is maintained to always be a valid row
      self.rows[row0 + usize::from(self.scroll_bottom)].wrap(false);
    }
  }

  pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
    let bottom = bottom.min(self.size().rows - 1);
    if top < bottom {
      self.scroll_top = top;
      self.scroll_bottom = bottom;
    } else {
      self.scroll_top = 0;
      self.scroll_bottom = self.size().rows - 1;
    }
    self.pos.row = self.scroll_top;
    self.pos.col = 0;
  }

  fn in_scroll_region(&self) -> bool {
    self.pos.row >= self.scroll_top && self.pos.row <= self.scroll_bottom
  }

  fn scroll_region_active(&self) -> bool {
    self.scroll_top != 0 || self.scroll_bottom != self.size.rows - 1
  }

  pub fn set_origin_mode(&mut self, mode: bool) {
    self.origin_mode = mode;
    self.set_pos(Pos { row: 0, col: 0 });
  }

  pub fn row_inc_clamp(&mut self, count: u16) {
    let in_scroll_region = self.in_scroll_region();
    self.pos.row = self.pos.row.saturating_add(count);
    self.row_clamp_bottom(in_scroll_region);
  }

  pub fn row_inc_scroll(&mut self, count: u16) -> u16 {
    let in_scroll_region = self.in_scroll_region();
    self.pos.row = self.pos.row.saturating_add(count);
    let lines = self.row_clamp_bottom(in_scroll_region);
    if in_scroll_region {
      self.scroll_up(lines);
      lines
    } else {
      0
    }
  }

  pub fn row_dec_clamp(&mut self, count: u16) {
    let in_scroll_region = self.in_scroll_region();
    self.pos.row = self.pos.row.saturating_sub(count);
    self.row_clamp_top(in_scroll_region);
  }

  pub fn row_dec_scroll(&mut self, count: u16) {
    let in_scroll_region = self.in_scroll_region();
    // need to account for clamping by both row_clamp_top and by
    // saturating_sub
    let extra_lines = if count > self.pos.row {
      count - self.pos.row
    } else {
      0
    };
    self.pos.row = self.pos.row.saturating_sub(count);
    let lines = self.row_clamp_top(in_scroll_region);
    self.scroll_down(lines + extra_lines);
  }

  pub fn row_set(&mut self, i: u16) {
    self.pos.row = i;
    self.row_clamp();
  }

  pub fn col_inc(&mut self, count: u16) {
    self.pos.col = self.pos.col.saturating_add(count);
  }

  pub fn col_inc_clamp(&mut self, count: u16) {
    self.pos.col = self.pos.col.saturating_add(count);
    self.col_clamp();
  }

  pub fn col_dec(&mut self, count: u16) {
    self.pos.col = self.pos.col.saturating_sub(count);
  }

  pub fn col_tab(&mut self) {
    self.pos.col -= self.pos.col % 8;
    self.pos.col += 8;
    self.col_clamp();
  }

  pub fn col_set(&mut self, i: u16) {
    self.pos.col = i;
    self.col_clamp();
  }

  pub fn col_wrap(&mut self, width: u16, wrap: bool) {
    if self.pos.col > self.size.cols - width {
      let mut prev_pos = self.pos;
      self.pos.col = 0;
      let scrolled = self.row_inc_scroll(1);
      prev_pos.row -= scrolled;
      let new_pos = self.pos;
      self
        .drawing_row_mut(prev_pos.row)
        // we assume self.pos.row is always valid, and so prev_pos.row
        // must be valid because it is always less than or equal to
        // self.pos.row
        .unwrap()
        .wrap(wrap && prev_pos.row + 1 == new_pos.row);
    }
  }

  fn row_clamp_top(&mut self, limit_to_scroll_region: bool) -> u16 {
    if limit_to_scroll_region && self.pos.row < self.scroll_top {
      let rows = self.scroll_top - self.pos.row;
      self.pos.row = self.scroll_top;
      rows
    } else {
      0
    }
  }

  fn row_clamp_bottom(&mut self, limit_to_scroll_region: bool) -> u16 {
    let bottom = if limit_to_scroll_region {
      self.scroll_bottom
    } else {
      self.size.rows - 1
    };
    if self.pos.row > bottom {
      let rows = self.pos.row - bottom;
      self.pos.row = bottom;
      rows
    } else {
      0
    }
  }

  fn row_clamp(&mut self) {
    if self.pos.row > self.size.rows - 1 {
      self.pos.row = self.size.rows - 1;
    }
  }

  fn col_clamp(&mut self) {
    if self.pos.col > self.size.cols - 1 {
      self.pos.col = self.size.cols - 1;
    }
  }

  pub(crate) fn is_wide_continuation(&self, pos: Pos) -> bool {
    self
      .rows
      .get(self.row0() + pos.row as usize)
      .map_or(false, |r| r.is_wide_continuation(pos.col))
  }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Size {
  pub rows: u16,
  pub cols: u16,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Pos {
  pub row: u16,
  pub col: u16,
}
