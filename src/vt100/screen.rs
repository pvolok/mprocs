use std::fmt::Debug;

use crate::vt100::{attrs::Attrs, Color};
use compact_str::CompactString;
use termwiz::escape::csi::CursorStyle;
use unicode_width::UnicodeWidthChar as _;

use super::grid::Size;

const MODE_APPLICATION_KEYPAD: u8 = 0b0000_0001;
const MODE_APPLICATION_CURSOR: u8 = 0b0000_0010;
const MODE_HIDE_CURSOR: u8 = 0b0000_0100;
const MODE_ALTERNATE_SCREEN: u8 = 0b0000_1000;
const MODE_BRACKETED_PASTE: u8 = 0b0001_0000;

#[derive(Clone, Debug)]
pub enum CharSet {
  Ascii,
  Uk,
  DecLineDrawing,
}

/// The xterm mouse handling mode currently in use.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MouseProtocolMode {
  /// Mouse handling is disabled.
  None,

  /// Mouse button events should be reported on button press. Also known as
  /// X10 mouse mode.
  /// On/off: `CSI ? 9 h` / `CSI ? 9 l`
  Press,

  /// Mouse button events should be reported on button press and release.
  /// Also known as VT200 mouse mode.
  /// On/off: `CSI ? 1000 h` / `CSI ? 1000 l`
  PressRelease,

  /// On/off: `CSI ? 1001 h` / `CSI ? 1001 l`
  // Highlight,
  //
  /// Mouse button events should be reported on button press and release, as
  /// well as when the mouse moves between cells while a button is held
  /// down.
  /// On/off: `CSI ? 1002 h` / `CSI ? 1002 l`
  ButtonMotion,

  /// Mouse button events should be reported on button press and release,
  /// and mouse motion events should be reported when the mouse moves
  /// between cells regardless of whether a button is held down or not.
  /// On/off: `CSI ? 1003 h` / `CSI ? 1003 l`
  AnyMotion,
  // DecLocator,
}

impl Default for MouseProtocolMode {
  fn default() -> Self {
    Self::None
  }
}

/// The encoding to use for the enabled `MouseProtocolMode`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum MouseProtocolEncoding {
  /// Default single-printable-byte encoding.
  #[default]
  Default,

  /// UTF-8-based encoding.
  Utf8,

  /// SGR-like encoding.
  Sgr,
  // Urxvt,
}

/// Represents the overall terminal state.
#[derive(Clone, Debug)]
pub struct Screen {
  feed_buf: Vec<u8>,

  grid: crate::vt100::grid::Grid,
  alternate_grid: crate::vt100::grid::Grid,

  attrs: crate::vt100::attrs::Attrs,
  saved_attrs: crate::vt100::attrs::Attrs,

  cursor_style: CursorStyle,

  modes: u8,
  mouse_protocol_mode: MouseProtocolMode,
  mouse_protocol_encoding: MouseProtocolEncoding,

  g0: CharSet,
  g1: CharSet,
  shift_out: bool,

  /// If true, writing a character inserts a new cell
  insert: bool,

  audible_bell_count: usize,
  visual_bell_count: usize,

  errors: usize,
}

impl Screen {
  #[must_use]
  pub fn get_selected_text(
    &self,
    low_x: i32,
    low_y: i32,
    high_x: i32,
    high_y: i32,
  ) -> String {
    self.grid().get_selected_text(low_x, low_y, high_x, high_y)
  }

  pub(crate) fn new(
    size: crate::vt100::grid::Size,
    scrollback_len: usize,
  ) -> Self {
    let grid = crate::vt100::grid::Grid::new(size, scrollback_len);
    Self {
      feed_buf: Vec::new(),

      grid,
      alternate_grid: crate::vt100::grid::Grid::new(size, 0),

      attrs: crate::vt100::attrs::Attrs::default(),
      saved_attrs: crate::vt100::attrs::Attrs::default(),

      cursor_style: CursorStyle::Default,

      modes: 0,
      mouse_protocol_mode: MouseProtocolMode::default(),
      mouse_protocol_encoding: MouseProtocolEncoding::default(),

      g0: CharSet::Ascii,
      g1: CharSet::Ascii,
      shift_out: false,

      insert: false,

      audible_bell_count: 0,
      visual_bell_count: 0,

      errors: 0,
    }
  }

  pub(crate) fn set_size(&mut self, rows: u16, cols: u16) {
    self.grid.set_size(crate::vt100::grid::Size { rows, cols });
    self
      .alternate_grid
      .set_size(crate::vt100::grid::Size { rows, cols });
  }

  /// Returns the current size of the terminal.
  ///
  /// The return value will be (rows, cols).
  #[must_use]
  pub fn size(&self) -> Size {
    self.grid().size()
  }

  /// Returns the current position in the scrollback.
  ///
  /// This position indicates the offset from the top of the screen, and is
  /// `0` when the normal screen is in view.
  #[must_use]
  pub fn scrollback(&self) -> usize {
    self.grid().scrollback()
  }

  #[must_use]
  pub fn scrollback_len(&self) -> usize {
    self.grid().scrollback_len()
  }

  pub fn set_scrollback(&mut self, rows: usize) {
    self.grid_mut().set_scrollback(rows);
  }

  pub fn scroll_screen_up(&mut self, n: usize) {
    let pos = usize::saturating_add(self.scrollback(), n);
    self.set_scrollback(pos);
  }

  pub fn scroll_screen_down(&mut self, n: usize) {
    let pos = usize::saturating_sub(self.scrollback(), n);
    self.set_scrollback(pos);
  }

  /// Returns the current cursor position of the terminal.
  ///
  /// The return value will be (row, col).
  #[must_use]
  pub fn cursor_position(&self) -> (u16, u16) {
    let pos = self.grid().pos();
    (pos.row, pos.col)
  }

  /// Returns the `Cell` object at the given location in the terminal, if it
  /// exists.
  #[must_use]
  pub fn cell(&self, row: u16, col: u16) -> Option<&crate::vt100::cell::Cell> {
    self
      .grid()
      .visible_cell(crate::vt100::grid::Pos { row, col })
  }

  #[must_use]
  pub fn cursor_style(&self) -> CursorStyle {
    self.cursor_style
  }

  /// Returns whether the terminal should be in application cursor mode.
  #[must_use]
  pub fn application_cursor(&self) -> bool {
    self.mode(MODE_APPLICATION_CURSOR)
  }

  /// Returns whether the terminal should be in hide cursor mode.
  #[must_use]
  pub fn hide_cursor(&self) -> bool {
    self.mode(MODE_HIDE_CURSOR)
  }

  /// Returns the currently active `MouseProtocolMode`
  #[must_use]
  pub fn mouse_protocol_mode(&self) -> MouseProtocolMode {
    self.mouse_protocol_mode
  }

  fn grid(&self) -> &crate::vt100::grid::Grid {
    if self.mode(MODE_ALTERNATE_SCREEN) {
      &self.alternate_grid
    } else {
      &self.grid
    }
  }

  fn grid_mut(&mut self) -> &mut crate::vt100::grid::Grid {
    if self.mode(MODE_ALTERNATE_SCREEN) {
      &mut self.alternate_grid
    } else {
      &mut self.grid
    }
  }

  fn enter_alternate_grid(&mut self) {
    self.grid_mut().set_scrollback(0);
    self.set_mode(MODE_ALTERNATE_SCREEN);
  }

  fn exit_alternate_grid(&mut self) {
    self.clear_mode(MODE_ALTERNATE_SCREEN);
  }

  fn save_cursor(&mut self) {
    self.grid_mut().save_cursor();
    self.saved_attrs = self.attrs;
  }

  fn restore_cursor(&mut self) {
    self.grid_mut().restore_cursor();
    self.attrs = self.saved_attrs;
  }

  fn set_mode(&mut self, mode: u8) {
    self.modes |= mode;
  }

  fn clear_mode(&mut self, mode: u8) {
    self.modes &= !mode;
  }

  fn mode(&self, mode: u8) -> bool {
    self.modes & mode != 0
  }

  fn set_mouse_mode(&mut self, mode: MouseProtocolMode) {
    self.mouse_protocol_mode = mode;
  }

  fn clear_mouse_mode(&mut self, mode: MouseProtocolMode) {
    if self.mouse_protocol_mode == mode {
      self.mouse_protocol_mode = MouseProtocolMode::default();
    }
  }

  fn set_mouse_encoding(&mut self, encoding: MouseProtocolEncoding) {
    self.mouse_protocol_encoding = encoding;
  }

  fn clear_mouse_encoding(&mut self, encoding: MouseProtocolEncoding) {
    if self.mouse_protocol_encoding == encoding {
      self.mouse_protocol_encoding = MouseProtocolEncoding::default();
    }
  }
}

impl Screen {
  fn text(&mut self, c: char) {
    let pos = self.grid().pos();
    let size = self.grid().size();
    let attrs = self.attrs;

    let width = c.width();
    if width.is_none() && (u32::from(c)) < 256 {
      // don't even try to draw control characters
      return;
    }
    let width = width
      .unwrap_or(1)
      .try_into()
      // width() can only return 0, 1, or 2
      .unwrap();

    // it doesn't make any sense to wrap if the last column in a row
    // didn't already have contents. don't try to handle the case where a
    // character wraps because there was only one column left in the
    // previous row - literally everything handles this case differently,
    // and this is tmux behavior (and also the simplest). i'm open to
    // reconsidering this behavior, but only with a really good reason
    // (xterm handles this by introducing the concept of triple width
    // cells, which i really don't want to do).
    let mut wrap = false;
    if pos.col > size.cols - width {
      let last_cell_pos = crate::vt100::grid::Pos {
        row: pos.row,
        col: size.cols - 1,
      };
      let last_cell = self
        .grid()
        .drawing_cell(last_cell_pos)
        // pos.row is valid, since it comes directly from
        // self.grid().pos() which we assume to always have a valid
        // row value. size.cols - 1 is also always a valid column.
        .unwrap();
      if last_cell.has_contents()
        || self.grid().is_wide_continuation(last_cell_pos)
      {
        wrap = true;
      }
    }
    self.grid_mut().col_wrap(width, wrap);
    let pos = self.grid().pos();

    if width == 0 {
      if pos.col > 0 {
        let prev_cell_pos = crate::vt100::grid::Pos {
          row: pos.row,
          col: pos.col - 1,
        };
        let prev_cell_pos = if self.grid().is_wide_continuation(prev_cell_pos) {
          crate::vt100::grid::Pos {
            row: pos.row,
            col: pos.col - 2,
          }
        } else {
          prev_cell_pos
        };
        let prev_cell = self
          .grid_mut()
          .drawing_cell_mut(prev_cell_pos)
          // pos.row is valid, since it comes directly from
          // self.grid().pos() which we assume to always have a
          // valid row value. pos.col - 1 is valid because we just
          // checked for pos.col > 0.
          // pos.col - 2 is valid because pos.col - 1 is a wide continuation
          .unwrap();
        prev_cell.append(c);
      } else if pos.row > 0 {
        let prev_row = self
          .grid()
          .drawing_row(pos.row - 1)
          // pos.row is valid, since it comes directly from
          // self.grid().pos() which we assume to always have a
          // valid row value. pos.row - 1 is valid because we just
          // checked for pos.row > 0.
          .unwrap();
        if prev_row.wrapped() {
          let prev_cell_pos = crate::vt100::grid::Pos {
            row: pos.row - 1,
            col: size.cols - 1,
          };
          let prev_cell_pos = if self.grid().is_wide_continuation(prev_cell_pos)
          {
            crate::vt100::grid::Pos {
              row: pos.row - 1,
              col: size.cols - 2,
            }
          } else {
            prev_cell_pos
          };
          let prev_cell = self
            .grid_mut()
            .drawing_cell_mut(prev_cell_pos)
            // pos.row is valid, since it comes directly from
            // self.grid().pos() which we assume to always
            // have a valid row value. pos.row - 1 is valid
            // because we just checked for pos.row > 0. col of
            // size.cols - 2 is valid because the cell at
            // size.cols - 1 is a wide continuation character,
            // so it must have the first half of the wide
            // character before it.
            .unwrap();
          prev_cell.append(c);
        }
      }
    } else {
      if self.grid().is_wide_continuation(pos) {
        let prev_cell = self
          .grid_mut()
          .drawing_cell_mut(crate::vt100::grid::Pos {
            row: pos.row,
            col: pos.col - 1,
          })
          // pos.row is valid because we assume self.grid().pos() to
          // always have a valid row value. pos.col is valid because
          // we called col_wrap() immediately before this, which
          // ensures that self.grid().pos().col has a valid value.
          // pos.col - 1 is valid because the cell at pos.col is a
          // wide continuation character, so it must have the first
          // half of the wide character before it.
          .unwrap();
        prev_cell.clear(attrs);
      }

      if self
        .grid()
        .drawing_cell(pos)
        // pos.row is valid because we assume self.grid().pos() to
        // always have a valid row value. pos.col is valid because we
        // called col_wrap() immediately before this, which ensures
        // that self.grid().pos().col has a valid value.
        .unwrap()
        .is_wide()
      {
        if let Some(next_cell) =
          self.grid_mut().drawing_cell_mut(crate::vt100::grid::Pos {
            row: pos.row,
            col: pos.col + 1,
          })
        {
          next_cell.set(' ', attrs);
        }
      }

      let cell = self
        .grid_mut()
        .drawing_cell_mut(pos)
        // pos.row is valid because we assume self.grid().pos() to
        // always have a valid row value. pos.col is valid because we
        // called col_wrap() immediately before this, which ensures
        // that self.grid().pos().col has a valid value.
        .unwrap();
      cell.set(c, attrs);
      self.grid_mut().col_inc(1);
      if width > 1 {
        let pos = self.grid().pos();
        if self
          .grid()
          .drawing_cell(pos)
          // pos.row is valid because we assume self.grid().pos() to
          // always have a valid row value. pos.col is valid because
          // we called col_wrap() earlier, which ensures that
          // self.grid().pos().col has a valid value. this is true
          // even though we just called col_inc, because this branch
          // only happens if width > 1, and col_wrap takes width
          // into account.
          .unwrap()
          .is_wide()
        {
          let next_next_pos = crate::vt100::grid::Pos {
            row: pos.row,
            col: pos.col + 1,
          };
          let next_next_cell = self
            .grid_mut()
            .drawing_cell_mut(next_next_pos)
            // pos.row is valid because we assume
            // self.grid().pos() to always have a valid row value.
            // pos.col is valid because we called col_wrap()
            // earlier, which ensures that self.grid().pos().col
            // has a valid value. this is true even though we just
            // called col_inc, because this branch only happens if
            // width > 1, and col_wrap takes width into account.
            // pos.col + 1 is valid because the cell at pos.col is
            // wide, and so it must have the second half of the
            // wide character after it.
            .unwrap();
          next_next_cell.clear(attrs);
          if next_next_pos.col == size.cols - 1 {
            self
              .grid_mut()
              .drawing_row_mut(pos.row)
              // we assume self.grid().pos().row is always valid
              .unwrap()
              .wrap(false);
          }
        }
        let next_cell = self
          .grid_mut()
          .drawing_cell_mut(pos)
          // pos.row is valid because we assume self.grid().pos() to
          // always have a valid row value. pos.col is valid because
          // we called col_wrap() earlier, which ensures that
          // self.grid().pos().col has a valid value. this is true
          // even though we just called col_inc, because this branch
          // only happens if width > 1, and col_wrap takes width
          // into account.
          .unwrap();
        next_cell.clear(crate::vt100::attrs::Attrs::default());
        self.grid_mut().col_inc(1);
      }
    }
  }

  // control codes

  fn bel(&mut self) {
    self.audible_bell_count += 1;
  }

  fn tab(&mut self) {
    self.grid_mut().col_tab();
  }

  // escape codes

  // ESC 7
  fn decsc(&mut self) {
    self.save_cursor();
  }

  // ESC 8
  fn decrc(&mut self) {
    self.restore_cursor();
  }

  // ESC =
  fn deckpam(&mut self) {
    self.set_mode(MODE_APPLICATION_KEYPAD);
  }

  // ESC M
  fn ri(&mut self) {
    self.grid_mut().row_dec_scroll(1);
  }

  // ESC c
  fn ris(&mut self) {
    let audible_bell_count = self.audible_bell_count;
    let visual_bell_count = self.visual_bell_count;
    let errors = self.errors;

    *self = Self::new(self.grid.size(), self.grid.scrollback_len());

    self.audible_bell_count = audible_bell_count;
    self.visual_bell_count = visual_bell_count;
    self.errors = errors;
  }

  // ESC g
  fn vb(&mut self) {
    self.visual_bell_count += 1;
  }

  // csi codes

  // CSI @
  fn ich(&mut self, count: u16) {
    self.grid_mut().insert_cells(count);
  }

  // CSI J
  fn ed(&mut self, mode: u16) {
    let attrs = self.attrs;
    match mode {
      0 => self.grid_mut().erase_all_forward(attrs),
      1 => self.grid_mut().erase_all_backward(attrs),
      2 => self.grid_mut().erase_all(attrs),
      n => {
        log::warn!("Unhandled ED mode: {n}");
      }
    }
  }

  // CSI ? J
  fn decsed(&mut self, mode: u16) {
    self.ed(mode);
  }

  // CSI K
  fn el(&mut self, mode: u16) {
    let attrs = self.attrs;
    match mode {
      0 => self.grid_mut().erase_row_forward(attrs),
      1 => self.grid_mut().erase_row_backward(attrs),
      2 => self.grid_mut().erase_row(attrs),
      n => {
        log::debug!("unhandled EL mode: {n}");
      }
    }
  }

  // CSI ? K
  fn decsel(&mut self, mode: u16) {
    self.el(mode);
  }
}

#[derive(Clone, Debug)]
pub enum VtEvent {
  Bell,
  Reply(CompactString),
}

impl Screen {
  /// <https://man7.org/linux/man-pages/man4/console_codes.4.html>
  /// <https://en.wikipedia.org/wiki/ANSI_escape_code>
  /// <https://terminalguide.namepad.de/seq>/
  /// <https://vt100.net/docs/vt510-rm/contents.html>
  /// <https://xtermjs.org/docs/api/vtfeatures/>
  /// <https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences>
  /// <https://bjh21.me.uk/all-escapes/all-escapes.txt>
  pub fn process(&mut self, data: &[u8], events: &mut Vec<VtEvent>) {
    self.feed_buf.extend_from_slice(data);
    let buf = std::mem::take(&mut self.feed_buf);

    let mut pos = 0;
    let mut consumed = pos;
    'process: while pos < buf.len() {
      let seq_start = pos;
      pos += 1;
      match buf[seq_start] {
        0x07 => {
          // BELL
          events.push(VtEvent::Bell);
        }
        0x08 => {
          // BS - Backspace
          self.grid_mut().col_dec(1);
        }
        0x09 => {
          // HT - Horizontal Tabulation
          self.tab();
        }
        0x0A => {
          // LF - Line Feed
          self.grid_mut().row_inc_scroll(1);
        }
        0x0B => {
          // VT - Vertical Tabulation
          self.grid_mut().row_inc_scroll(1);
        }
        0x0C => {
          // FF - Form Feed
          self.grid_mut().row_inc_scroll(1);
        }
        0x0D => {
          // CR - Carriage Return
          self.grid_mut().col_set(0);
        }
        0x0E => {
          // SO - Shift Out
          self.shift_out = true;
        }
        0x0F => {
          // SI - Shift In
          self.shift_out = false;
        }
        0x1B => {
          if pos >= buf.len() {
            break;
          }
          pos += 1;
          match buf[pos - 1] {
            first @ 0x20..=0x2F => {
              // nF sequences
              // ESC [0x20-0x2F]+ [0x30-0x7E]
              let start = pos;
              'seq: loop {
                if pos >= buf.len() {
                  break 'process;
                }
                if buf[pos] >= 0x30 && buf[pos] <= 0x7E {
                  pos += 1;
                  break 'seq;
                }
                pos += 1;
              }
              let params = &buf[start..pos];
              match first {
                b'(' | b')' | b'*' | b'+' => {
                  // ESC ( rest - Setup G0 charset with 94 characters
                  // ESC ) rest - Setup G1 charset with 94 characters
                  // ESC * rest - Setup G2 charset with 94 characters
                  // ESC + rest - Setup G3 charset with 94 characters
                  // https://terminalguide.namepad.de/seq/
                  let charset = match params[0] {
                    b'A' => {
                      // UK
                      Some(CharSet::Uk)
                    }
                    b'B' => {
                      // ASCII
                      Some(CharSet::Ascii)
                    }
                    b'0' => {
                      // DEC Special Character and Line Drawing Set
                      Some(CharSet::DecLineDrawing)
                    }
                    _ => None,
                  };
                  if let Some(charset) = charset {
                    match first {
                      b'(' => self.g0 = charset,
                      b')' => self.g1 = charset,
                      _ => (),
                    }
                  }
                }
                _ => {
                  log::warn!(
                    "Ignored nF: ESC {}",
                    String::from_utf8_lossy(&buf[start - 1..pos]),
                  );
                }
              }
            }
            b'7' => {
              self.save_cursor();
            }
            b'8' => {
              self.restore_cursor();
            }
            b'=' => {
              // DECKPAM
              self.set_mode(MODE_APPLICATION_KEYPAD);
            }
            b'>' => {
              // DECKPNM
              self.clear_mode(MODE_APPLICATION_KEYPAD);
            }
            b'@' => {
              if pos >= buf.len() {
                break;
              }
              // Consume one byte
              pos += 1;
            }
            b'M' => {
              // RI - Reverse Index
              self.ri();
            }
            b'P' => {
              // Device Control String
              let start = pos;
              'dcs: loop {
                if pos + 2 > buf.len() {
                  break 'process;
                }
                if &buf[pos..pos + 2] == b"\x1b\\" {
                  let _dcs = &buf[start..pos];
                  // TODO: Handle DCS
                  pos += 2;
                  break 'dcs;
                }
                pos += 1;
              }
            }
            b'[' => {
              let params_start = pos;
              while pos < buf.len() && (0x30..=0x3F).contains(&buf[pos]) {
                pos += 1;
              }
              let params = &buf[params_start..pos];

              let intermediate_start = pos;
              while pos < buf.len() && (0x20..=0x2F).contains(&buf[pos]) {
                pos += 1;
              }
              let intermediate = &buf[intermediate_start..pos];

              if pos >= buf.len() {
                break;
              }
              let final_ = &buf[pos];
              if (0x40..=0x7E).contains(final_) {
                pos += 1;
                let params = str::from_utf8(params).unwrap_or_default();
                let intermediate =
                  str::from_utf8(intermediate).unwrap_or_default();
                self.process_csi(events, params, intermediate, *final_);
              } else {
                let seq1 = &buf[seq_start + 1..pos + 1];
                log::error!(
                  "Corrupt CSI sequence: ESC {}  - {:?}",
                  String::from_utf8_lossy(seq1),
                  seq1,
                );
                // Only consume the first '0x1B' byte.
                pos = seq_start + 1;
              }
            }
            b']' => {
              // Operating System Command
              let start = pos;
              'osc: loop {
                if pos >= buf.len() {
                  break 'process;
                }
                let mut s = None;
                if buf[pos] == 0x07 || buf[pos] == 0x9C {
                  s = Some(&buf[start..pos]);
                  pos += 1;
                } else if buf.get(pos..pos + 2) == Some(b"\x1b\\") {
                  s = Some(&buf[start..pos]);
                  pos += 2;
                }
                if let Some(_s) = s {
                  // TODO: Handle OSC
                  break 'osc;
                }

                pos += 1;
              }
            }
            b'X' | b'^' | b'_' => {
              // ESC X - Start of String
              // ESC ^ - Privacy Message
              // ESC _ - Application Program Command
              let start = pos;
              'cmd: loop {
                if pos + 2 > buf.len() {
                  break 'process;
                }
                if &buf[pos..pos + 2] == b"\x1b\\" {
                  let _cmd = &buf[start..pos];
                  pos += 2;
                  break 'cmd;
                }
                pos += 1;
              }
            }
            c => {
              log::warn!(
                "Unhandled ESC {} ({:?})",
                c,
                char::from_u32(c.into())
              );
            }
          }
        }
        0x8D => {
          // RI
          self.ri();
        }
        first_byte => {
          let char_len = utf8_char_len(first_byte);
          if char_len == 0 {
            // Ignore invalid byte.
          } else if seq_start + char_len <= buf.len() {
            let char_bytes = &buf[seq_start..seq_start + char_len];
            let char = str::from_utf8(char_bytes);
            match char {
              Ok(s) => {
                pos = seq_start + char_len;
                let char = s.chars().next().unwrap();
                self.text(char);
              }
              Err(e) => {
                log::error!("Invalid utf-8 char: {char_bytes:?} {e}");
              }
            }
          } else {
            break;
          }
        }
      }

      consumed = pos;
    }

    self.feed_buf = buf;
    self.feed_buf.drain(0..consumed);
  }

  fn process_csi(
    &mut self,
    events: &mut Vec<VtEvent>,
    params: &str,
    intermediate: &str,
    final_: u8,
  ) {
    let full_params = params;
    let (pref, bare_params) = if params.starts_with(['<', '=', '>', '?']) {
      (&params[..1], &params[1..])
    } else {
      ("", params)
    };
    match (pref, bare_params, intermediate, final_) {
      ("?", _, "", b'J') => {
        // DECSED - Selective Erase Display
        // https://terminalguide.namepad.de/seq/csi_cj__p/
        let mode = params.parse().unwrap_or(0);
        self.decsed(mode);
      }
      ("?", _, "", b'K') => {
        // DECSEL - Selective Erase Line
        // https://terminalguide.namepad.de/seq/csi_ck__p/
        let mode = params.parse().unwrap_or(0);
        self.decsel(mode);
      }
      ("", _, "", b'@') => {
        // ICH - Insert Character
        // https://terminalguide.namepad.de/seq/csi_x40_at/
        let amount = params.parse().unwrap_or(1);
        self.ich(amount);
      }
      ("", _, "", b'A') => {
        // CUU - Cursor Up
        // https://terminalguide.namepad.de/seq/csi_ca/
        let count = params.parse().unwrap_or(1).max(1);
        self.grid_mut().row_dec_clamp(count);
      }
      ("", _, "", b'B') => {
        // CUD - Cursor Down
        // https://terminalguide.namepad.de/seq/csi_cb/
        let count = params.parse().unwrap_or(1).max(1);
        self.grid_mut().row_inc_clamp(count);
      }
      ("", _, "", b'C') => {
        // CUF - Cursor Right
        // https://terminalguide.namepad.de/seq/csi_cc/
        let count = params.parse().unwrap_or(1).max(1);
        self.grid_mut().col_inc_clamp(count);
      }
      ("", _, "", b'D') => {
        // CUB - Cursor Left
        // https://terminalguide.namepad.de/seq/csi_cd/
        let count = params.parse().unwrap_or(1).max(1);
        self.grid_mut().col_dec(count);
      }
      ("", _, "", b'E') => {
        // CNL - Cursor Next Line
        // https://terminalguide.namepad.de/seq/csi_ce/
        let count = params.parse().unwrap_or(1).max(1);
        self.grid_mut().row_inc_clamp(count);
        self.grid_mut().col_set(0);
      }
      ("", _, "", b'F') => {
        // CPL - Cursor Previous Line
        // https://terminalguide.namepad.de/seq/csi_cf/
        let count = params.parse().unwrap_or(1).max(1);
        self.grid_mut().row_dec_clamp(count);
        self.grid_mut().col_set(0);
      }
      ("", _, "", b'G') => {
        // CHA - Cursor Horizontal Absolute
        // https://terminalguide.namepad.de/seq/csi_cg/
        let column = params.parse().unwrap_or(1).max(1) - 1;
        self.grid_mut().col_set(column);
      }
      ("", _, "", b'H') => {
        // CUP - Cursor Position
        // https://terminalguide.namepad.de/seq/csi_ch/
        let mut params = params.split(';');
        let row = params.next().unwrap_or("1").parse().unwrap_or(1).max(1) - 1;
        let col = params.next().unwrap_or("1").parse().unwrap_or(1).max(1) - 1;
        self
          .grid_mut()
          .set_pos(crate::vt100::grid::Pos { row, col });
      }
      ("", _, "", b'J') => {
        // ED - Erase Display
        // https://terminalguide.namepad.de/seq/csi_cj/
        let mode = params.parse().unwrap_or(0);
        self.ed(mode);
      }
      ("", _, "", b'K') => {
        // EL - Erase Line
        // https://terminalguide.namepad.de/seq/csi_ck/
        let mode = params.parse().unwrap_or(0);
        self.el(mode);
      }
      ("", _, "", b'L') => {
        // IL - Insert Line
        // https://terminalguide.namepad.de/seq/csi_cl/
        let amount = params.parse().unwrap_or(1);
        self.grid_mut().insert_lines(amount);
      }
      ("", _, "", b'M') => {
        // DL - Delete Line
        // https://terminalguide.namepad.de/seq/csi_cm/
        let amount = params.parse().unwrap_or(1).max(1);
        let attrs = self.attrs;
        self.grid_mut().delete_lines(amount, attrs);
      }
      ("", _, "", b'P') => {
        // DCH - Delete Character
        // https://terminalguide.namepad.de/seq/csi_cp/
        let amount = params.parse().unwrap_or(1).max(1);
        self.grid_mut().delete_cells(amount);
      }
      ("", _, "", b'S') => {
        // SU - Scroll Up
        // https://terminalguide.namepad.de/seq/csi_cs/
        let amount = params.parse().unwrap_or(1);
        self.grid_mut().scroll_up(amount);
      }
      ("", _, "", b'T') => {
        // SD - Scroll Down
        // https://terminalguide.namepad.de/seq/csi_ct_1param/
        //  or
        // Track Mouse
        // https://terminalguide.namepad.de/seq/csi_ct_5param/
        if let Ok(amount) = params.parse() {
          // It's SD if only one parameter
          self.grid_mut().scroll_down(amount);
        } else {
          log::warn!("Ignored CSI {params} T");
        }
      }
      ("", _, "", b'X') => {
        // ECH - Erase Character
        // https://terminalguide.namepad.de/seq/csi_cx/
        let amount = params.parse().unwrap_or(1).max(1);
        let attrs = self.attrs;
        self.grid_mut().erase_cells(amount, attrs);
      }
      ("", _, "", b'c') => {
        // DA1 - Primary Device Attributes
        // https://terminalguide.namepad.de/seq/csi_sc/
        // https://vt100.net/docs/vt510-rm/DA1.html
        // let p1 = params.parse().unwrap_or(0);

        // 4 - Sixel
        // 6 - Selective erase
        // 22 - ANSI color, vt525
        // 52 - Clipboard access
        events.push(VtEvent::Reply("\x1b[?65;6;22;52c".into()));
      }
      ("", _, "", b'd') => {
        // VPA - Vertical Position Absolute
        // https://terminalguide.namepad.de/seq/csi_sd/
        let row = params.parse().unwrap_or(1).max(1) - 1;
        self.grid_mut().row_set(row);
      }
      ("", _, "", b'f') => {
        // HVP - Horizontal and Vertical Position
        // https://terminalguide.namepad.de/seq/csi_sf/
        let mut params = params.split(';');
        let y = params.next().unwrap_or("1").parse().unwrap_or(1).max(1) - 1;
        let x = params.next().unwrap_or("1").parse().unwrap_or(1).max(1) - 1;
        self
          .grid_mut()
          .set_pos(crate::vt100::grid::Pos { row: y, col: x });
      }
      ("", _, "", b'`') => {
        // HPA - Horizontal Position Absolute
        // https://terminalguide.namepad.de/seq/csi_x60_backtick/
        let column = params.parse().unwrap_or(1).max(1) - 1;
        self.grid_mut().col_set(column);
      }
      ("", _, "", b'm') => {
        let mut params = params.split(';');
        match params.next().unwrap_or("0").parse().unwrap_or(0) {
          0 => {
            // Reset
            self.attrs = Attrs::default();
          }
          1 => {
            // Bold
            self.attrs.set_bold(true);
          }
          2 => {
            // Dim
            self.attrs.set_bold(false);
          }
          3 => {
            // Italic
            self.attrs.set_italic(true);
          }
          4 => {
            // Underline
            self.attrs.set_underline(true);
          }
          5 => {
            // Slow blink
            // TODO
          }
          6 => {
            // Rapid blink
            // TODO
          }
          7 => {
            // Invert
            self.attrs.set_inverse(true);
          }
          9 => {
            // Crossed-out
            // TODO
          }
          21 => {
            // Doubly underlined
            self.attrs.set_underline(true);
          }
          22 => {
            // Normal intensity
            self.attrs.set_bold(false);
          }
          23 => {
            // Not italic
            self.attrs.set_italic(false);
          }
          24 => {
            // Not underlined
            self.attrs.set_underline(false);
          }
          25 => {
            // Not blinking
            // TODO
          }
          27 => {
            // Not reversed
            self.attrs.set_inverse(false);
          }
          29 => {
            // Not crossed-out
            // TODO
          }
          n @ 30..=37 => {
            self.attrs.fgcolor = Color::Idx(n - 30);
          }
          38 => {
            self.attrs.fgcolor = parse_sgr_color(params);
          }
          39 => {
            self.attrs.fgcolor = Color::Default;
          }
          n @ 40..=47 => {
            self.attrs.bgcolor = Color::Idx(n - 40);
          }
          48 => {
            self.attrs.bgcolor = parse_sgr_color(params);
          }
          49 => {
            self.attrs.bgcolor = Color::Default;
          }
          n @ 90..=97 => {
            self.attrs.fgcolor = Color::Idx(n - 90 + 8);
          }
          n @ 100..=107 => {
            self.attrs.bgcolor = Color::Idx(n - 100 + 8);
          }
          n => {
            log::warn!("Ignored SGR: {}", n);
          }
        }
      }
      (_, _, "", b'h' | b'l') => {
        let set = final_ == b'h';
        // https://terminalguide.namepad.de/mode/
        match params {
          "?1" => {
            // DECCKM
            if set {
              self.set_mode(MODE_APPLICATION_CURSOR);
            } else {
              self.clear_mode(MODE_APPLICATION_CURSOR);
            }
          }
          "4" => {
            self.insert = set;
          }
          "?6" => {
            self.grid_mut().set_origin_mode(set);
          }
          "?9" => {
            // Mouse Click-Only Tracking (X10_MOUSE)
            if set {
              self.set_mouse_mode(MouseProtocolMode::Press);
            } else {
              self.clear_mouse_mode(MouseProtocolMode::Press);
            }
          }
          "?25" => {
            // DECTCEM
            if set {
              self.clear_mode(MODE_HIDE_CURSOR);
            } else {
              self.set_mode(MODE_HIDE_CURSOR);
            }
          }
          "34" => {
            // DECRLM - Cursor direction, right to left
            // https://vt100.net/docs/vt510-rm/DECRLM.html
            // Not supported
          }
          "?47" => {
            // Alternate Screen Buffer (ALTBUF)
            if set {
              self.enter_alternate_grid();
            } else {
              self.exit_alternate_grid();
            }
          }
          "?1000" => {
            if set {
              self.set_mouse_mode(MouseProtocolMode::PressRelease);
            } else {
              self.clear_mouse_mode(MouseProtocolMode::PressRelease);
            }
          }
          "?1002" => {
            if set {
              self.set_mouse_mode(MouseProtocolMode::ButtonMotion);
            } else {
              self.clear_mouse_mode(MouseProtocolMode::ButtonMotion);
            }
          }
          "?1003" => {
            if set {
              self.set_mouse_mode(MouseProtocolMode::AnyMotion);
            } else {
              self.clear_mouse_mode(MouseProtocolMode::AnyMotion);
            }
          }
          "?1005" => {
            if set {
              self.set_mouse_encoding(MouseProtocolEncoding::Utf8);
            } else {
              self.clear_mouse_encoding(MouseProtocolEncoding::Utf8);
            }
          }
          "?1006" => {
            if set {
              self.set_mouse_encoding(MouseProtocolEncoding::Sgr);
            } else {
              self.clear_mouse_encoding(MouseProtocolEncoding::Sgr);
            }
          }
          "?1049" => {
            // Alternate Screen Buffer, With Cursor Save and Clear on Enter
            if set {
              self.decsc();
              self.alternate_grid.clear();
              self.enter_alternate_grid();
            } else {
              self.exit_alternate_grid();
              self.decrc();
            }
          }
          "?2004" => {
            // Bracketed Paste Mode
            if set {
              self.set_mode(MODE_BRACKETED_PASTE);
            } else {
              self.clear_mode(MODE_BRACKETED_PASTE);
            }
          }
          _ => csi_todo(full_params, intermediate, final_),
        }
      }
      ("", _, "", b'n') => {
        // DSR - Device Status Report
        match params {
          "6" => {
            // CPR - Request Cursor Position Report
            // https://terminalguide.namepad.de/seq/csi_sn-6/
            let pos = self.grid().pos();
            let s = compact_str::format_compact!(
              "\x1b[{};{}R",
              pos.row + 1,
              pos.col + 1
            );
            events.push(VtEvent::Reply(s));
          }
          n => {
            log::warn!("Ignored DSR: {}", n);
          }
        }
      }
      ("", _, " ", b'q') => {
        // DECSCUSR - Select Cursor Style
        // https://terminalguide.namepad.de/seq/csi_sq_t_space/
        let cursor_style = match params {
          "0" => CursorStyle::Default,
          "1" => CursorStyle::BlinkingBlock,
          "2" => CursorStyle::SteadyBlock,
          "3" => CursorStyle::BlinkingUnderline,
          "4" => CursorStyle::SteadyUnderline,
          "5" => CursorStyle::BlinkingBar,
          "6" => CursorStyle::SteadyBar,
          _ => CursorStyle::Default,
        };
        self.cursor_style = cursor_style;
      }
      ("", _, "", b'r') => {
        // DECSTBM - Set Top and Bottom Margins
        // https://terminalguide.namepad.de/seq/csi_sr/
        let top = params.parse().unwrap_or(1).max(1) - 1;
        let bottom = params.parse().unwrap_or(1).max(1) - 1;
        self.grid_mut().set_scroll_region(top, bottom);
      }
      _ => csi_todo(full_params, intermediate, final_),
    }
  }
}

fn csi_todo(params: &str, intermediate: &str, final_: u8) {
  log::warn!(
    "CSI not implemented: ESC [ {} {} {}",
    params,
    intermediate,
    final_ as char
  );
}

fn parse_sgr_color(mut params: std::str::Split<'_, char>) -> Color {
  match params.next().unwrap_or("2") {
    "2" => {
      let r = params.next().unwrap_or("0").parse().unwrap_or(0);
      let g = params.next().unwrap_or("0").parse().unwrap_or(0);
      let b = params.next().unwrap_or("0").parse().unwrap_or(0);
      Color::Rgb(r, g, b)
    }
    "5" => {
      let n = params.next().unwrap_or("0").parse().unwrap_or(0);
      Color::Idx(n)
    }
    _ => Color::Default,
  }
}

fn utf8_char_len(first_byte: u8) -> usize {
  match first_byte {
    // https://en.wikipedia.org/wiki/UTF-8#Description
    (0x00..=0x7F) => 1, // 0xxxxxxx
    (0xC0..=0xDF) => 2, // 110xxxxx 10xxxxxx
    (0xE0..=0xEF) => 3, // 1110xxxx 10xxxxxx 10xxxxxx
    (0xF0..=0xF7) => 4, // 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
    (0x80..=0xBF) | (0xF8..=0xFF) => 0,
  }
}
