use std::ops::Range;

use crate::vt100::grid::Rect;

#[derive(Default)]
pub struct ListState {
  selected: i32,
  top_index: i32,

  count: i32,
  height: i32,
}

impl ListState {
  pub fn fit(&mut self, area: Rect, count: usize) {
    self.count = count as i32;
    self.height = area.height as i32;
    self.autoscroll();
    // let bottom_gap = self.top_index + area.height - items.len();
    // if bottom_gap > 0 {
    //   self.top_index -= bottom_gap.min(self.top_index);
    // }
  }

  //
  //          0
  // +-----+  1
  // |     | _2_
  // |     |  3
  // |     |  4
  // +-----+  5
  //
  // top_index = 2
  //
  fn autoscroll(&mut self) {
    // If selected item is above area.
    self.top_index = self.top_index.min(self.selected);
    // If selected item is below area.
    self.top_index = self.top_index.max(self.selected + 1 - self.height);
    // Restrict bottom gap.
    self.top_index = self.top_index.min((self.count - self.height).max(0));
  }

  #[allow(dead_code)]
  pub fn select_relative(&mut self, n: i32) {
    let mut selected = self.selected.saturating_add(n) % self.count as i32;
    if selected < 0 {
      selected = self.count as i32 - selected;
    }
    self.selected = selected;
    self.autoscroll();
  }

  pub fn select(&mut self, index: usize) {
    let index = index as i32;
    self.selected = index.min(self.count - 1);
    self.autoscroll();
  }

  pub fn visible_range(&self) -> Range<usize> {
    let start = self.top_index as usize;
    let end = (self.top_index + self.height).min(self.count) as usize;
    start..end
  }

  pub fn selected(&self) -> usize {
    self.selected as usize
  }
}
