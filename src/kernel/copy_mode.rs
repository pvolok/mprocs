use crate::term::Screen;

pub struct CopyState {
  snapshot: Screen,
  start: Pos,
  end: Option<Pos>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pos {
  pub y: i32,
  pub x: i32,
}

#[derive(Clone, Copy, Debug)]
pub enum CopyMove {
  Up,
  Down,
  Left,
  Right,
}

impl CopyState {
  /// Freezes `screen` and starts copy mode with the cursor on the bottom row.
  pub fn new(screen: Screen) -> Self {
    let y = (screen.size().height as i32 - 1).max(0);
    CopyState {
      snapshot: screen,
      start: Pos { y, x: 0 },
      end: None,
    }
  }

  pub fn snapshot(&self) -> &Screen {
    &self.snapshot
  }

  pub fn start(&self) -> Pos {
    self.start
  }

  pub fn end(&self) -> Option<Pos> {
    self.end
  }

  /// The position the cursor is currently driving: the selection cursor once a
  /// selection is started, otherwise the lone anchor.
  pub fn cursor(&self) -> Pos {
    self.end.unwrap_or(self.start)
  }

  fn cursor_mut(&mut self) -> &mut Pos {
    self.end.as_mut().unwrap_or(&mut self.start)
  }

  pub fn move_cursor(&mut self, dir: CopyMove) {
    let width = self.snapshot.size().width as i32;
    let height = self.snapshot.size().height as i32;
    let scrollback_len = self.snapshot.scrollback_len() as i32;
    let pos = self.cursor_mut();
    match dir {
      CopyMove::Up => {
        if pos.y > -scrollback_len {
          pos.y -= 1;
        }
      }
      CopyMove::Down => {
        if pos.y + 1 < height {
          pos.y += 1;
        }
      }
      CopyMove::Left => {
        if pos.x > 0 {
          pos.x -= 1;
        }
      }
      CopyMove::Right => {
        if pos.x + 1 < width {
          pos.x += 1;
        }
      }
    }
  }

  /// Anchors the selection at the current cursor and begins extending it.
  pub fn begin_selection(&mut self) {
    if self.end.is_none() {
      self.end = Some(self.start);
    }
  }

  /// Converts a present-screen cell (row, col) to a copy-mode position,
  /// accounting for the current scroll offset.
  pub fn pos_at(&self, row: u16, col: u16) -> Pos {
    Pos {
      y: row as i32 - self.snapshot.scrollback() as i32,
      x: col as i32,
    }
  }

  /// Places the selection anchor at `pos`, clearing any active selection.
  pub fn set_anchor(&mut self, pos: Pos) {
    self.start = pos;
    self.end = None;
  }

  /// Extends the selection to `pos`.
  pub fn set_extent(&mut self, pos: Pos) {
    self.end = Some(pos);
  }

  pub fn scroll_up(&mut self, n: usize) {
    self.snapshot.scroll_screen_up(n);
  }

  pub fn scroll_down(&mut self, n: usize) {
    self.snapshot.scroll_screen_down(n);
  }

  /// Current scroll offset (rows above the live bottom).
  pub fn scrollback(&self) -> usize {
    self.snapshot.scrollback()
  }

  /// Extracts the selected text, or `None` if no selection has been started.
  pub fn selected_text(&self) -> Option<String> {
    let end = self.end?;
    let (low, high) = Pos::to_low_high(self.start, end);
    Some(
      self
        .snapshot
        .get_selected_text(low.x, low.y, high.x, high.y),
    )
  }
}

impl Pos {
  pub fn to_low_high(a: Pos, b: Pos) -> (Pos, Pos) {
    if a.y < b.y || (a.y == b.y && a.x < b.x) {
      (a, b)
    } else {
      (b, a)
    }
  }

  /// Whether `target` falls inside the inclusive selection rectangle (in
  /// reading order) spanned by `start`..`end`.
  pub fn within(start: Pos, end: Pos, target: Pos) -> bool {
    let (low, high) = Pos::to_low_high(start, end);
    let Pos { y, x } = target;
    if y > low.y {
      y < high.y || (y == high.y && x <= high.x)
    } else if y == low.y {
      if y < high.y {
        x >= low.x
      } else if y == high.y {
        x >= low.x && x <= high.x
      } else {
        false
      }
    } else {
      false
    }
  }
}
