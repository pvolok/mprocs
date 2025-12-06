use std::fmt::Debug;

use crate::vt100::{attrs::Attrs, TermReplySender};
use compact_str::{CompactString, ToCompactString};
use termwiz::escape::{
  csi::{
    CsiParam, Cursor, CursorStyle, DecPrivateMode, DecPrivateModeCode, Edit,
    EraseInDisplay, EraseInLine, Sgr, TerminalMode, TerminalModeCode, Window,
  },
  Action, ControlCode, DeviceControlMode, Esc, EscCode, OneBased,
  OperatingSystemCommand, CSI,
};
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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MouseProtocolEncoding {
  /// Default single-printable-byte encoding.
  Default,

  /// UTF-8-based encoding.
  Utf8,

  /// SGR-like encoding.
  Sgr,
  // Urxvt,
}

impl Default for MouseProtocolEncoding {
  fn default() -> Self {
    Self::Default
  }
}

/// Represents the overall terminal state.
#[derive(Clone, Debug)]
pub struct Screen<Reply: TermReplySender> {
  reply_sender: Reply,

  grid: crate::vt100::grid::Grid,
  alternate_grid: crate::vt100::grid::Grid,

  attrs: crate::vt100::attrs::Attrs,
  saved_attrs: crate::vt100::attrs::Attrs,

  title: String,
  icon_name: String,

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

impl<Reply: TermReplySender> Screen<Reply> {
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
    reply_sender: Reply,
  ) -> Self {
    let grid = crate::vt100::grid::Grid::new(size, scrollback_len);
    Self {
      reply_sender,
      grid,
      alternate_grid: crate::vt100::grid::Grid::new(size, 0),

      attrs: crate::vt100::attrs::Attrs::default(),
      saved_attrs: crate::vt100::attrs::Attrs::default(),

      title: String::default(),
      icon_name: String::default(),

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

impl<Reply: TermReplySender + Clone> Screen<Reply> {
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
    let title = self.title.clone();
    let icon_name = self.icon_name.clone();
    let audible_bell_count = self.audible_bell_count;
    let visual_bell_count = self.visual_bell_count;
    let errors = self.errors;

    *self = Self::new(
      self.grid.size(),
      self.grid.scrollback_len(),
      self.reply_sender.clone(),
    );

    self.title = title;
    self.icon_name = icon_name;
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
        log::debug!("unhandled ED mode: {n}");
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

macro_rules! skip {
  ($fmt:expr) => {
    {
      use std::fmt::Write;
      let mut output = String::new();
      write!(output, $fmt).unwrap();
      log::debug!("Skip seq: {}", output);
    }
  };
  ($fmt:expr, $($arg:tt)*) => {
    {
      use std::fmt::Write;
      let mut output = String::new();
      write!(output, $fmt, $($arg)*).unwrap();
      log::debug!("Skip seq: {}", output);
    }
  };
}

#[allow(clippy::match_same_arms, clippy::semicolon_if_nothing_returned)]
impl<Reply: TermReplySender + Clone> Screen<Reply> {
  pub fn handle_action(&mut self, action: Action) {
    match action {
      Action::Print(c) => self.text(c),
      Action::PrintString(s) => s.chars().for_each(|c| self.text(c)),
      Action::Control(code) => self.handle_control(code),
      Action::DeviceControl(mode) => self.handle_device_control(mode),
      Action::OperatingSystemCommand(cmd) => self.handle_os_command(*cmd),
      Action::CSI(csi) => self.handle_csi(csi),
      Action::Esc(esc) => self.handle_esc(esc),
      Action::Sixel(_) => (),
      Action::XtGetTcap(names) => self.handle_xt_get_tcap(names),
      Action::KittyImage(_) => (),
    }
  }

  fn handle_control(&mut self, code: ControlCode) {
    match code {
      ControlCode::Null => {}
      ControlCode::StartOfHeading => skip!("StartOfHeading"),
      ControlCode::StartOfText => skip!("StartOfText"),
      ControlCode::EndOfText => skip!("EndOfText"),
      ControlCode::EndOfTransmission => skip!("EndOfTransmission"),
      ControlCode::Enquiry => skip!("Enquiry"),
      ControlCode::Acknowledge => skip!("Acknowledge"),
      ControlCode::Bell => self.bel(),
      ControlCode::Backspace => self.grid_mut().col_dec(1),
      ControlCode::HorizontalTab => self.tab(),
      ControlCode::LineFeed => {
        self.grid_mut().row_inc_scroll(1);
      }
      ControlCode::VerticalTab => {
        self.grid_mut().row_inc_scroll(1);
      }
      ControlCode::FormFeed => {
        self.grid_mut().row_inc_scroll(1);
      }
      ControlCode::CarriageReturn => {
        self.grid_mut().col_set(0);
      }
      ControlCode::ShiftOut => self.shift_out = true,
      ControlCode::ShiftIn => self.shift_out = false,
      ControlCode::DataLinkEscape => skip!("DataLinkEscape"),
      ControlCode::DeviceControlOne => skip!("DeviceControlOne"),
      ControlCode::DeviceControlTwo => skip!("DeviceControlTwo"),
      ControlCode::DeviceControlThree => skip!("DeviceControlThree"),
      ControlCode::DeviceControlFour => skip!("DeviceControlFour"),
      ControlCode::NegativeAcknowledge => skip!("NegativeAcknowledge"),
      ControlCode::SynchronousIdle => skip!("SynchronousIdle"),
      ControlCode::EndOfTransmissionBlock => {
        skip!("EndOfTransmissionBlock")
      }
      ControlCode::Cancel => skip!("Cancel"),
      ControlCode::EndOfMedium => skip!("EndOfMedium"),
      ControlCode::Substitute => skip!("Substitute"),
      ControlCode::Escape => skip!("Escape"),
      ControlCode::FileSeparator => skip!("FileSeparator"),
      ControlCode::GroupSeparator => skip!("GroupSeparator"),
      ControlCode::RecordSeparator => skip!("RecordSeparator"),
      ControlCode::UnitSeparator => skip!("UnitSeparator"),
      ControlCode::BPH => skip!("BPH"),
      ControlCode::NBH => skip!("NBH"),
      ControlCode::IND => skip!("IND"),
      ControlCode::NEL => skip!("NEL"),
      ControlCode::SSA => skip!("SSA"),
      ControlCode::ESA => skip!("ESA"),
      ControlCode::HTS => skip!("HTS"),
      ControlCode::HTJ => skip!("HTJ"),
      ControlCode::VTS => skip!("VTS"),
      ControlCode::PLD => skip!("PLD"),
      ControlCode::PLU => skip!("PLU"),
      ControlCode::RI => self.ri(),
      ControlCode::SS2 => skip!("SS2"),
      ControlCode::SS3 => skip!("SS3"),
      ControlCode::DCS => skip!("DCS"),
      ControlCode::PU1 => skip!("PU1"),
      ControlCode::PU2 => skip!("PU2"),
      ControlCode::STS => skip!("STS"),
      ControlCode::CCH => skip!("CCH"),
      ControlCode::MW => skip!("MW"),
      ControlCode::SPA => skip!("SPA"),
      ControlCode::EPA => skip!("EPA"),
      ControlCode::SOS => skip!("SOS"),
      ControlCode::SCI => skip!("SCI"),
      ControlCode::CSI => skip!("CSI"),
      ControlCode::ST => skip!("ST"),
      ControlCode::OSC => skip!("OSC"),
      ControlCode::PM => skip!("PM"),
      ControlCode::APC => skip!("APC"),
    }
  }

  fn handle_device_control(&mut self, _mode: DeviceControlMode) {
    skip!("DeviceControl");
  }

  fn handle_os_command(&mut self, cmd: OperatingSystemCommand) {
    match cmd {
      OperatingSystemCommand::SetIconNameAndWindowTitle(icon_and_title) => {
        self.icon_name.clone_from(&icon_and_title);
        self.icon_name = icon_and_title.clone();
        self.title = icon_and_title;
      }
      OperatingSystemCommand::SetWindowTitle(title) => self.title = title,
      OperatingSystemCommand::SetWindowTitleSun(_) => {
        skip!("SetWindowTitleSun")
      }
      OperatingSystemCommand::SetIconName(icon) => self.icon_name = icon,
      OperatingSystemCommand::SetIconNameSun(_) => skip!("SetIconNameSun"),
      OperatingSystemCommand::SetHyperlink(_) => skip!("SetHyperlink"),
      OperatingSystemCommand::ClearSelection(_) => skip!("ClearSelection"),
      OperatingSystemCommand::QuerySelection(_) => skip!("QuerySelection"),
      OperatingSystemCommand::SetSelection(_, _) => skip!("SetSelection"),
      OperatingSystemCommand::SystemNotification(_) => {
        skip!("SystemNotification")
      }
      OperatingSystemCommand::ITermProprietary(_) => skip!("ITermProprietary"),
      OperatingSystemCommand::FinalTermSemanticPrompt(p) => {
        skip!("FinalTermSemanticPrompt {:?}", p)
      }
      OperatingSystemCommand::ChangeColorNumber(_) => {
        skip!("ChangeColorNumber")
      }
      OperatingSystemCommand::ChangeDynamicColors(first_color, colors) => {
        skip!("ChangeDynamicColors {:?} {:?}", first_color, colors)
      }
      OperatingSystemCommand::ResetDynamicColor(_) => {
        skip!("ResetDynamicColor")
      }
      OperatingSystemCommand::CurrentWorkingDirectory(_) => {
        skip!("CurrentWorkingDirectory")
      }
      OperatingSystemCommand::ResetColors(_) => skip!("ResetColors"),
      OperatingSystemCommand::RxvtExtension(_) => skip!("RxvtExtension"),
      OperatingSystemCommand::ConEmuProgress(_progress) => {
        skip!("ConEmuProgress")
      }
      OperatingSystemCommand::Unspecified(data) => {
        let strings: Vec<_> = data
          .into_iter()
          .map(|bytes| String::from_utf8_lossy(bytes.as_slice()).to_string())
          .collect();
        skip!("OSC: Unspecified {:?}", strings);
      }
    }
  }

  fn handle_csi(&mut self, csi: CSI) {
    match csi {
      CSI::Sgr(sgr) => match sgr {
        Sgr::Reset => self.attrs = Attrs::default(),
        Sgr::Intensity(level) => match level {
          termwiz::cell::Intensity::Normal => self.attrs.set_bold(false),
          termwiz::cell::Intensity::Bold => self.attrs.set_bold(true),
          termwiz::cell::Intensity::Half => self.attrs.set_bold(true),
        },
        Sgr::Underline(mode) => match mode {
          termwiz::cell::Underline::None => self.attrs.set_underline(false),
          termwiz::cell::Underline::Single
          | termwiz::cell::Underline::Double
          | termwiz::cell::Underline::Curly
          | termwiz::cell::Underline::Dotted
          | termwiz::cell::Underline::Dashed => self.attrs.set_underline(true),
        },
        Sgr::UnderlineColor(_) => skip!("UnderlineColor"),
        Sgr::Blink(_) => skip!("Blink"),
        Sgr::Italic(mode) => self.attrs.set_italic(mode),
        Sgr::Inverse(mode) => self.attrs.set_inverse(mode),
        Sgr::Invisible(_) => skip!("Invisible"),
        Sgr::StrikeThrough(_) => skip!("StrikeThrough"),
        Sgr::Font(_) => skip!("Font"),
        Sgr::Foreground(color) => self.attrs.fgcolor = color.into(),
        Sgr::Background(color) => self.attrs.bgcolor = color.into(),
        Sgr::Overline(_) => skip!("Overline"),
        Sgr::VerticalAlign(_) => skip!("VerticalAlign"),
      },
      CSI::Cursor(cursor) => match cursor {
        Cursor::BackwardTabulation(_) => skip!("BackwardTabulation"),
        Cursor::TabulationClear(_) => skip!("TabulationClear"),
        Cursor::CharacterAbsolute(pos) => {
          self.grid_mut().col_set(pos.as_zero_based() as u16);
        }
        Cursor::CharacterPositionAbsolute(_) => {
          skip!("CharacterPositionAbsolute")
        }
        Cursor::CharacterPositionBackward(_) => {
          skip!("CharacterPositionBackward")
        }
        Cursor::CharacterPositionForward(_) => {
          skip!("CharacterPositionForward")
        }
        Cursor::CharacterAndLinePosition { line, col } => {
          self.grid_mut().set_pos(crate::vt100::grid::Pos {
            row: line.as_zero_based() as u16,
            col: col.as_zero_based() as u16,
          });
        }
        Cursor::LinePositionAbsolute(row) => {
          self.grid_mut().row_set((row - 1) as u16);
        }
        Cursor::LinePositionBackward(_) => skip!("LinePositionBackward"),
        Cursor::LinePositionForward(_) => skip!("LinePositionForward"),
        Cursor::ForwardTabulation(_) => skip!("ForwardTabulation"),
        Cursor::NextLine(_) => skip!("NextLine"),
        Cursor::PrecedingLine(_) => skip!("PrecedingLine"),
        Cursor::ActivePositionReport { line: _, col: _ } => {
          skip!("ActivePositionReport")
        }
        Cursor::RequestActivePositionReport => {
          let pos = self.grid().pos();
          let es = CSI::Cursor(Cursor::ActivePositionReport {
            line: OneBased::from_zero_based(pos.row.into()),
            col: OneBased::from_zero_based(pos.col.into()),
          });
          self.reply_sender.reply(es.to_compact_string());
        }
        Cursor::SaveCursor => skip!("SaveCursor"),
        Cursor::RestoreCursor => skip!("RestoreCursor"),
        Cursor::TabulationControl(_) => skip!("TabulationControl"),
        Cursor::Left(count) => {
          self.grid_mut().col_dec(count as u16);
        }
        Cursor::Down(count) => {
          self.grid_mut().row_inc_clamp(count as u16);
        }
        Cursor::Right(count) => {
          self.grid_mut().col_inc_clamp(count as u16);
        }
        Cursor::Position { line, col } => {
          self.grid_mut().set_pos(crate::vt100::grid::Pos {
            row: line.as_zero_based() as u16,
            col: col.as_zero_based() as u16,
          });
        }
        Cursor::Up(count) => {
          self.grid_mut().row_dec_clamp(count as u16);
        }
        Cursor::LineTabulation(_) => skip!("LineTabulation"),
        Cursor::SetTopAndBottomMargins { top, bottom } => {
          self.grid_mut().set_scroll_region(
            top.as_zero_based() as u16,
            bottom.as_zero_based() as u16,
          )
        }
        Cursor::SetLeftAndRightMargins { left: _, right: _ } => {
          skip!("SetLeftAndRightMargins")
        }
        Cursor::CursorStyle(style) => {
          self.cursor_style = style;
        }
      },
      CSI::Edit(edit) => match edit {
        Edit::DeleteCharacter(count) => {
          self.grid_mut().delete_cells(count as u16);
        }
        Edit::DeleteLine(count) => {
          self.grid_mut().delete_lines(count as u16);
        }
        Edit::EraseCharacter(count) => {
          let attrs = self.attrs;
          self.grid_mut().erase_cells(count as u16, attrs);
        }
        Edit::EraseInLine(mode) => {
          let attrs = self.attrs;
          match mode {
            EraseInLine::EraseToEndOfLine => {
              self.grid_mut().erase_row_forward(attrs)
            }
            EraseInLine::EraseToStartOfLine => {
              self.grid_mut().erase_row_backward(attrs)
            }
            EraseInLine::EraseLine => self.grid_mut().erase_row(attrs),
          }
        }
        Edit::InsertCharacter(count) => {
          self.ich(u16::try_from(count).unwrap_or_default());
        }
        Edit::InsertLine(count) => {
          self.grid_mut().insert_lines(count as u16);
        }
        Edit::ScrollDown(count) => {
          self.grid_mut().scroll_down(count as u16);
        }
        Edit::ScrollUp(count) => {
          self.grid_mut().scroll_up(count as u16);
        }
        Edit::EraseInDisplay(mode) => {
          let attrs = self.attrs;
          match mode {
            EraseInDisplay::EraseToEndOfDisplay => {
              self.grid_mut().erase_all_forward(attrs)
            }
            EraseInDisplay::EraseToStartOfDisplay => {
              self.grid_mut().erase_all_backward(attrs)
            }
            EraseInDisplay::EraseDisplay => self.grid_mut().erase_all(attrs),
            EraseInDisplay::EraseScrollback => skip!("EraseScrollback"),
          }
        }
        Edit::Repeat(_) => skip!("Repeat"),
      },
      CSI::Mode(mode) => match mode {
        termwiz::escape::csi::Mode::SetDecPrivateMode(pmode) => match pmode {
          DecPrivateMode::Code(code) => match code {
            DecPrivateModeCode::ApplicationCursorKeys => {
              self.set_mode(MODE_APPLICATION_CURSOR)
            }
            DecPrivateModeCode::DecAnsiMode => skip!("DecAnsiMode"),
            DecPrivateModeCode::Select132Columns => {
              skip!("Select132Columns")
            }
            DecPrivateModeCode::SmoothScroll => skip!("SmoothScroll"),
            DecPrivateModeCode::ReverseVideo => skip!("ReverseVideo"),
            DecPrivateModeCode::OriginMode => {
              self.grid_mut().set_origin_mode(true)
            }
            DecPrivateModeCode::AutoWrap => skip!("AutoWrap"),
            DecPrivateModeCode::AutoRepeat => skip!("AutoRepeat"),
            DecPrivateModeCode::StartBlinkingCursor => {
              skip!("StartBlinkingCursor")
            }
            DecPrivateModeCode::ShowCursor => self.clear_mode(MODE_HIDE_CURSOR),
            DecPrivateModeCode::ReverseWraparound => {
              skip!("ReverseWraparound")
            }
            DecPrivateModeCode::LeftRightMarginMode => {
              skip!("LeftRightMarginMode")
            }
            DecPrivateModeCode::SixelDisplayMode => {
              skip!("SixelDisplayMode")
            }
            DecPrivateModeCode::MouseTracking => {
              self.set_mouse_mode(MouseProtocolMode::PressRelease)
            }
            DecPrivateModeCode::HighlightMouseTracking => {
              skip!("HighlightMouseTracking")
            }
            DecPrivateModeCode::ButtonEventMouse => {
              self.set_mouse_mode(MouseProtocolMode::ButtonMotion)
            }
            DecPrivateModeCode::AnyEventMouse => {
              self.set_mouse_mode(MouseProtocolMode::AnyMotion)
            }
            DecPrivateModeCode::FocusTracking => skip!("FocusTracking"),
            DecPrivateModeCode::Utf8Mouse => {
              self.set_mouse_encoding(MouseProtocolEncoding::Utf8)
            }
            DecPrivateModeCode::SGRMouse => {
              self.set_mouse_encoding(MouseProtocolEncoding::Sgr)
            }
            DecPrivateModeCode::SGRPixelsMouse => skip!("SGRPixelsMouse"),
            DecPrivateModeCode::XTermMetaSendsEscape => {
              skip!("XTermMetaSendsEscape")
            }
            DecPrivateModeCode::XTermAltSendsEscape => {
              skip!("XTermAltSendsEscape")
            }
            DecPrivateModeCode::SaveCursor => skip!("SaveCursor"),
            DecPrivateModeCode::ClearAndEnableAlternateScreen => {
              self.decsc();
              self.alternate_grid.clear();
              self.enter_alternate_grid();
            }
            DecPrivateModeCode::EnableAlternateScreen => {
              self.enter_alternate_grid();
            }
            DecPrivateModeCode::OptEnableAlternateScreen => {
              skip!("OptEnableAlternateScreen")
            }
            DecPrivateModeCode::BracketedPaste => {
              self.set_mode(MODE_BRACKETED_PASTE);
            }
            DecPrivateModeCode::GraphemeClustering => {
              skip!("GraphemeClustering");
            }
            DecPrivateModeCode::UsePrivateColorRegistersForEachGraphic => {
              skip!("UsePrivateColorRegistersForEachGraphic")
            }
            DecPrivateModeCode::SynchronizedOutput => {
              skip!("SynchronizedOutput")
            }
            DecPrivateModeCode::MinTTYApplicationEscapeKeyMode => {
              skip!("MinTTYApplicationEscapeKeyMode")
            }
            DecPrivateModeCode::SixelScrollsRight => {
              skip!("SixelScrollsRight")
            }
            DecPrivateModeCode::Win32InputMode => skip!("Win32InputMode"),
          },
          DecPrivateMode::Unspecified(9) => {
            self.set_mouse_mode(MouseProtocolMode::Press)
          }
          DecPrivateMode::Unspecified(m) => {
            skip!("SetDecPrivateMode:Unspecified:{}", m)
          }
        },
        termwiz::escape::csi::Mode::ResetDecPrivateMode(pmode) => match pmode {
          DecPrivateMode::Code(code) => match code {
            DecPrivateModeCode::ApplicationCursorKeys => {
              self.clear_mode(MODE_APPLICATION_CURSOR)
            }
            DecPrivateModeCode::DecAnsiMode => skip!("DecAnsiMode"),
            DecPrivateModeCode::Select132Columns => {
              skip!("Select132Columns")
            }
            DecPrivateModeCode::SmoothScroll => skip!("SmoothScroll"),
            DecPrivateModeCode::ReverseVideo => skip!("ReverseVideo"),
            DecPrivateModeCode::OriginMode => {
              self.grid_mut().set_origin_mode(false)
            }
            DecPrivateModeCode::AutoWrap => skip!("AutoWrap"),
            DecPrivateModeCode::AutoRepeat => skip!("AutoRepeat"),
            DecPrivateModeCode::StartBlinkingCursor => {
              skip!("StartBlinkingCursor")
            }
            DecPrivateModeCode::ShowCursor => self.set_mode(MODE_HIDE_CURSOR),
            DecPrivateModeCode::ReverseWraparound => {
              skip!("ReverseWraparound")
            }
            DecPrivateModeCode::LeftRightMarginMode => {
              skip!("LeftRightMarginMode")
            }
            DecPrivateModeCode::SixelDisplayMode => {
              skip!("SixelDisplayMode")
            }
            DecPrivateModeCode::MouseTracking => {
              self.clear_mouse_mode(MouseProtocolMode::PressRelease)
            }
            DecPrivateModeCode::HighlightMouseTracking => {
              skip!("HighlightMouseTracking")
            }
            DecPrivateModeCode::ButtonEventMouse => {
              self.clear_mouse_mode(MouseProtocolMode::ButtonMotion)
            }
            DecPrivateModeCode::AnyEventMouse => {
              self.clear_mouse_mode(MouseProtocolMode::AnyMotion)
            }
            DecPrivateModeCode::FocusTracking => skip!("FocusTracking"),
            DecPrivateModeCode::Utf8Mouse => {
              self.clear_mouse_encoding(MouseProtocolEncoding::Utf8)
            }
            DecPrivateModeCode::SGRMouse => {
              self.clear_mouse_encoding(MouseProtocolEncoding::Sgr)
            }
            DecPrivateModeCode::SGRPixelsMouse => {
              skip!("SGRPixelsMouse")
            }
            DecPrivateModeCode::XTermMetaSendsEscape => {
              skip!("XTermMetaSendsEscape")
            }
            DecPrivateModeCode::XTermAltSendsEscape => {
              skip!("XTermAltSendsEscape")
            }
            DecPrivateModeCode::SaveCursor => skip!("SaveCursor"),
            DecPrivateModeCode::ClearAndEnableAlternateScreen => {
              self.exit_alternate_grid();
              self.decrc();
            }
            DecPrivateModeCode::EnableAlternateScreen => {
              self.exit_alternate_grid()
            }
            DecPrivateModeCode::OptEnableAlternateScreen => {
              skip!("OptEnableAlternateScreen")
            }
            DecPrivateModeCode::BracketedPaste => {
              self.clear_mode(MODE_BRACKETED_PASTE)
            }
            DecPrivateModeCode::GraphemeClustering => {
              skip!("GraphemeClustering");
            }
            DecPrivateModeCode::UsePrivateColorRegistersForEachGraphic => {
              skip!("UsePrivateColorRegistersForEachGraphic")
            }
            DecPrivateModeCode::SynchronizedOutput => {
              skip!("SynchronizedOutput")
            }
            DecPrivateModeCode::MinTTYApplicationEscapeKeyMode => {
              skip!("MinTTYApplicationEscapeKeyMode")
            }
            DecPrivateModeCode::SixelScrollsRight => {
              skip!("SixelScrollsRight")
            }
            DecPrivateModeCode::Win32InputMode => {
              skip!("Win32InputMode")
            }
          },
          DecPrivateMode::Unspecified(9) => {
            self.clear_mouse_mode(MouseProtocolMode::Press)
          }
          termwiz::escape::csi::DecPrivateMode::Unspecified(_) => {
            skip!("DecPrivateMode::Unspecified")
          }
        },
        termwiz::escape::csi::Mode::SaveDecPrivateMode(pmode) => match pmode {
          DecPrivateMode::Code(code) => match code {
            DecPrivateModeCode::ApplicationCursorKeys => {
              skip!("ApplicationCursorKeys")
            }
            DecPrivateModeCode::DecAnsiMode => skip!("DecAnsiMode"),
            DecPrivateModeCode::Select132Columns => {
              skip!("Select132Columns")
            }
            DecPrivateModeCode::SmoothScroll => skip!("SmoothScroll"),
            DecPrivateModeCode::ReverseVideo => skip!("ReverseVideo"),
            DecPrivateModeCode::OriginMode => skip!("OriginMode"),
            DecPrivateModeCode::AutoWrap => skip!("AutoWrap"),
            DecPrivateModeCode::AutoRepeat => skip!("AutoRepeat"),
            DecPrivateModeCode::StartBlinkingCursor => {
              skip!("StartBlinkingCursor")
            }
            DecPrivateModeCode::ShowCursor => skip!("ShowCursor"),
            DecPrivateModeCode::ReverseWraparound => {
              skip!("ReverseWraparound")
            }
            DecPrivateModeCode::LeftRightMarginMode => {
              skip!("LeftRightMarginMode")
            }
            DecPrivateModeCode::SixelDisplayMode => {
              skip!("SixelDisplayMode")
            }
            DecPrivateModeCode::MouseTracking => skip!("MouseTracking"),
            DecPrivateModeCode::HighlightMouseTracking => {
              skip!("HighlightMouseTracking")
            }
            DecPrivateModeCode::ButtonEventMouse => {
              skip!("ButtonEventMouse")
            }
            DecPrivateModeCode::AnyEventMouse => skip!("AnyEventMouse"),
            DecPrivateModeCode::FocusTracking => skip!("FocusTracking"),
            DecPrivateModeCode::Utf8Mouse => skip!("Utf8Mouse"),
            DecPrivateModeCode::SGRMouse => skip!("SGRMouse"),
            DecPrivateModeCode::SGRPixelsMouse => {
              skip!("SGRPixelsMouse")
            }
            DecPrivateModeCode::XTermMetaSendsEscape => {
              skip!("XTermMetaSendsEscape")
            }
            DecPrivateModeCode::XTermAltSendsEscape => {
              skip!("XTermAltSendsEscape")
            }
            DecPrivateModeCode::SaveCursor => skip!("SaveCursor"),
            DecPrivateModeCode::ClearAndEnableAlternateScreen => {
              skip!("ClearAndEnableAlternateScreen")
            }
            DecPrivateModeCode::EnableAlternateScreen => {
              skip!("EnableAlternateScreen")
            }
            DecPrivateModeCode::OptEnableAlternateScreen => {
              skip!("OptEnableAlternateScreen")
            }
            DecPrivateModeCode::BracketedPaste => {
              skip!("BracketedPaste")
            }
            DecPrivateModeCode::GraphemeClustering => {
              skip!("GraphemeClustering");
            }
            DecPrivateModeCode::UsePrivateColorRegistersForEachGraphic => {
              skip!("UsePrivateColorRegistersForEachGraphic")
            }
            DecPrivateModeCode::SynchronizedOutput => {
              skip!("SynchronizedOutput")
            }
            DecPrivateModeCode::MinTTYApplicationEscapeKeyMode => {
              skip!("MinTTYApplicationEscapeKeyMode")
            }
            DecPrivateModeCode::SixelScrollsRight => {
              skip!("SixelScrollsRight")
            }
            DecPrivateModeCode::Win32InputMode => {
              skip!("Win32InputMode")
            }
          },
          termwiz::escape::csi::DecPrivateMode::Unspecified(_) => todo!(),
        },
        termwiz::escape::csi::Mode::RestoreDecPrivateMode(_) => {
          skip!("RestoreDecPrivateMode")
        }
        termwiz::escape::csi::Mode::QueryDecPrivateMode(mode) => {
          skip!("QueryDecPrivateMode {:?}", mode)
        }
        termwiz::escape::csi::Mode::SetMode(mode) => match mode {
          TerminalMode::Code(code) => match code {
            TerminalModeCode::KeyboardAction => {
              skip!("TerminalModeCode::KeyboardAction")
            }
            TerminalModeCode::Insert => skip!("TerminalModeCode::Insert"),
            TerminalModeCode::BiDirectionalSupportMode => {
              skip!("TerminalModeCode::BiDirectionalSupportMode")
            }
            TerminalModeCode::SendReceive => {
              skip!("TerminalModeCode::SendReceive")
            }
            TerminalModeCode::AutomaticNewline => {
              skip!("TerminalModeCode::AutomaticNewline")
            }
            TerminalModeCode::ShowCursor => {
              skip!("TerminalModeCode::ShowCursor")
            }
          },
          TerminalMode::Unspecified(n) => {
            if n == 34 {
              // DECRLM - Cursor direction, right to left
            } else {
              skip!("SetMode -> TerminalMode::Unspecified({})", n);
            }
          }
        },
        termwiz::escape::csi::Mode::ResetMode(mode) => match mode {
          TerminalMode::Code(code) => match code {
            TerminalModeCode::KeyboardAction => {
              skip!("TerminalModeCode::KeyboardAction")
            }
            TerminalModeCode::Insert => self.insert = false,
            TerminalModeCode::BiDirectionalSupportMode => {
              skip!("TerminalModeCode::BiDirectionalSupportMode")
            }
            TerminalModeCode::SendReceive => {
              skip!("TerminalModeCode::SendReceive")
            }
            TerminalModeCode::AutomaticNewline => {
              skip!("TerminalModeCode::AutomaticNewline")
            }
            TerminalModeCode::ShowCursor => {
              skip!("TerminalModeCode::ShowCursor")
            }
          },
          TerminalMode::Unspecified(n) => {
            skip!("ResetMode -> TerminalMode::Unspecified({})", n)
          }
        },
        termwiz::escape::csi::Mode::QueryMode(_) => skip!("QueryMode"),
        termwiz::escape::csi::Mode::XtermKeyMode {
          resource: _,
          value: _,
        } => {
          skip!("XtermKeyMode")
        }
      },
      CSI::Device(device) => match &*device {
        termwiz::escape::csi::Device::DeviceAttributes(device_attributes) => {
          skip!("DeviceAttributes: {:?}", device_attributes);
        }
        termwiz::escape::csi::Device::SoftReset => {
          skip!("SoftReset");
        }
        termwiz::escape::csi::Device::RequestPrimaryDeviceAttributes => {
          // https://vt100.net/docs/vt510-rm/DA1.html

          let mut reply = CompactString::new("\x1b[?65"); // Vt500

          // ident.push_str(";4"); // Sixel graphics

          reply.push_str(";6"); // Selective erase

          // ident.push_str(";18"); // windowing extensions

          reply.push_str(";22"); // ANSI color, vt525
          reply.push_str(";52"); // Clipboard access
          reply.push('c');

          self.reply_sender.reply(reply);
        }
        termwiz::escape::csi::Device::RequestSecondaryDeviceAttributes => {
          skip!("RequestSecondaryDeviceAttributes");
        }
        termwiz::escape::csi::Device::RequestTertiaryDeviceAttributes => {
          skip!("RequestTertiaryDeviceAttributes");
        }
        termwiz::escape::csi::Device::StatusReport => {
          skip!("StatusReport");
        }
        termwiz::escape::csi::Device::RequestTerminalNameAndVersion => {
          skip!("RequestTerminalNameAndVersion");
        }
        termwiz::escape::csi::Device::RequestTerminalParameters(x) => {
          skip!("RequestTerminalParameters: {:?}", x);
        }
        termwiz::escape::csi::Device::XtSmGraphics(xt_sm_graphics) => {
          skip!("XtSmGraphics: {:?}", xt_sm_graphics);
        }
      },
      CSI::Mouse(mouse) => skip!("Mouse: {:?}", mouse),
      CSI::Window(win) => match *win {
        Window::DeIconify => skip!("DeIconify"),
        Window::Iconify => skip!("Iconify"),
        Window::MoveWindow { x: _, y: _ } => skip!("MoveWindow"),
        Window::ResizeWindowPixels {
          width: _,
          height: _,
        } => {
          skip!("ResizeWindowPixels")
        }
        Window::RaiseWindow => skip!("RaiseWindow"),
        Window::LowerWindow => skip!("LowerWindow"),
        Window::RefreshWindow => skip!("RefreshWindow"),
        Window::ResizeWindowCells {
          width: _,
          height: _,
        } => {
          skip!("ResizeWindowCells")
        }
        Window::RestoreMaximizedWindow => skip!("RestoreMaximizedWindow"),
        Window::MaximizeWindow => skip!("MaximizeWindow"),
        Window::MaximizeWindowVertically => {
          skip!("MaximizeWindowVertically")
        }
        Window::MaximizeWindowHorizontally => {
          skip!("MaximizeWindowHorizontally")
        }
        Window::UndoFullScreenMode => skip!("UndoFullScreenMode"),
        Window::ChangeToFullScreenMode => skip!("ChangeToFullScreenMode"),
        Window::ToggleFullScreen => skip!("ToggleFullScreen"),
        Window::ReportWindowState => skip!("ReportWindowState"),
        Window::ReportWindowPosition => skip!("ReportWindowPosition"),
        Window::ReportTextAreaPosition => skip!("ReportTextAreaPosition"),
        Window::ReportTextAreaSizePixels => {
          skip!("ReportTextAreaSizePixels")
        }
        Window::ReportWindowSizePixels => skip!("ReportWindowSizePixels"),
        Window::ReportScreenSizePixels => skip!("ReportScreenSizePixels"),
        Window::ReportCellSizePixels => skip!("ReportCellSizePixels"),
        Window::ReportCellSizePixelsResponse {
          width: _,
          height: _,
        } => {
          skip!("ReportCellSizePixelsResponse")
        }
        Window::ReportTextAreaSizeCells => {
          skip!("ReportTextAreaSizeCells")
        }
        Window::ReportScreenSizeCells => skip!("ReportScreenSizeCells"),
        Window::ReportIconLabel => skip!("ReportIconLabel"),
        Window::ReportWindowTitle => skip!("ReportWindowTitle"),
        Window::PushIconAndWindowTitle => skip!("PushIconAndWindowTitle"),
        Window::PushIconTitle => skip!("PushIconTitle"),
        Window::PushWindowTitle => skip!("PushWindowTitle"),
        Window::PopIconAndWindowTitle => skip!("PopIconAndWindowTitle"),
        Window::PopIconTitle => skip!("PopIconTitle"),
        Window::PopWindowTitle => skip!("PopWindowTitle"),
        Window::ChecksumRectangularArea {
          request_id: _,
          page_number: _,
          top: _,
          left: _,
          bottom: _,
          right: _,
        } => skip!("ChecksumRectangularArea"),
      },
      CSI::Keyboard(kb) => match kb {
        termwiz::escape::csi::Keyboard::SetKittyState { flags: _, mode: _ } => {
          skip!("SetKittyState")
        }
        termwiz::escape::csi::Keyboard::PushKittyState {
          flags: _,
          mode: _,
        } => {
          skip!("PushKittyState")
        }
        termwiz::escape::csi::Keyboard::PopKittyState(_) => {
          skip!("PopKittyState")
        }
        termwiz::escape::csi::Keyboard::QueryKittySupport => {
          skip!("QueryKittySupport")
        }
        termwiz::escape::csi::Keyboard::ReportKittyState(_) => {
          skip!("ReportKittyState")
        }
      },
      CSI::SelectCharacterPath(_, _) => skip!("SelectCharacterPath"),
      CSI::Unspecified(n) => {
        let handled = match (n.control, n.params.as_slice()) {
          ('J', [CsiParam::P(b'?')]) => {
            self.decsed(0);
            true
          }
          ('J', [CsiParam::P(b'?'), CsiParam::Integer(mode)]) => {
            self.decsed(u16::try_from(*mode).unwrap_or_default());
            true
          }
          ('K', [CsiParam::P(b'?')]) => {
            self.decsel(0);
            true
          }
          ('K', [CsiParam::P(b'?'), CsiParam::Integer(mode)]) => {
            self.decsel(u16::try_from(*mode).unwrap_or_default());
            true
          }
          _ => false,
        };
        if !handled {
          skip!("unspecified {}", n);
        }
      }
    }
  }

  fn handle_esc(&mut self, esc: Esc) {
    match esc {
      Esc::Code(code) => match code {
        EscCode::FullReset => self.ris(),
        EscCode::Index => skip!("Index"),
        EscCode::NextLine => skip!("NextLine"),
        EscCode::CursorPositionLowerLeft => {
          skip!("CursorPositionLowerLeft")
        }
        EscCode::HorizontalTabSet => skip!("HorizontalTabSet"),
        EscCode::ReverseIndex => self.ri(),
        EscCode::SingleShiftG2 => skip!("SingleShiftG2"),
        EscCode::SingleShiftG3 => skip!("SingleShiftG3"),
        EscCode::StartOfGuardedArea => skip!("StartOfGuardedArea"),
        EscCode::EndOfGuardedArea => skip!("EndOfGuardedArea"),
        EscCode::StartOfString => skip!("StartOfString"),
        EscCode::ReturnTerminalId => skip!("ReturnTerminalId"),
        EscCode::StringTerminator => skip!("StringTerminator"),
        EscCode::PrivacyMessage => skip!("PrivacyMessage"),
        EscCode::ApplicationProgramCommand => {
          skip!("ApplicationProgramCommand")
        }
        EscCode::TmuxTitle => skip!("TmuxTitle"),
        EscCode::DecBackIndex => skip!("DecBackIndex"),
        EscCode::DecSaveCursorPosition => self.save_cursor(),
        EscCode::DecRestoreCursorPosition => self.restore_cursor(),
        EscCode::DecApplicationKeyPad => self.deckpam(),
        EscCode::DecNormalKeyPad => self.clear_mode(MODE_APPLICATION_KEYPAD),
        EscCode::DecLineDrawingG0 => self.g0 = CharSet::DecLineDrawing,
        EscCode::UkCharacterSetG0 => self.g0 = CharSet::Uk,
        EscCode::AsciiCharacterSetG0 => self.g0 = CharSet::Ascii,
        EscCode::DecLineDrawingG1 => self.g1 = CharSet::DecLineDrawing,
        EscCode::UkCharacterSetG1 => self.g1 = CharSet::Uk,
        EscCode::AsciiCharacterSetG1 => self.g1 = CharSet::Ascii,
        EscCode::DecScreenAlignmentDisplay => {
          skip!("DecScreenAlignmentDisplay")
        }
        EscCode::DecDoubleHeightTopHalfLine => {
          skip!("DecDoubleHeightTopHalfLine")
        }
        EscCode::DecDoubleHeightBottomHalfLine => {
          skip!("DecDoubleHeightBottomHalfLine")
        }
        EscCode::DecSingleWidthLine => skip!("DecSingleWidthLine"),
        EscCode::DecDoubleWidthLine => skip!("DecDoubleWidthLine"),
        EscCode::ApplicationModeArrowUpPress => {
          skip!("ApplicationModeArrowUpPress")
        }
        EscCode::ApplicationModeArrowDownPress => {
          skip!("ApplicationModeArrowDownPress")
        }
        EscCode::ApplicationModeArrowRightPress => {
          skip!("ApplicationModeArrowRightPress")
        }
        EscCode::ApplicationModeArrowLeftPress => {
          skip!("ApplicationModeArrowLeftPress")
        }
        EscCode::ApplicationModeHomePress => {
          skip!("ApplicationModeHomePress")
        }
        EscCode::ApplicationModeEndPress => {
          skip!("ApplicationModeEndPress")
        }
        EscCode::F1Press => skip!("F1Press"),
        EscCode::F2Press => skip!("F2Press"),
        EscCode::F3Press => skip!("F3Press"),
        EscCode::F4Press => skip!("F4Press"),
      },
      Esc::Unspecified {
        intermediate,
        control,
      } => match (intermediate, control) {
        (None, b'g') => self.vb(),
        _ => {
          skip!("Unspecified esc: {:?} {}", intermediate, control);
        }
      },
    }
  }

  fn handle_xt_get_tcap(&mut self, _names: Vec<String>) {
    skip!("XtGetTcap");
  }
}
