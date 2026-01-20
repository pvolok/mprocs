use crate::vt100::TermReplySender;

/// A parser for terminal output which produces an in-memory representation of
/// the terminal contents.
pub struct Parser<Reply: TermReplySender + Clone> {
  pub screen: crate::vt100::screen::Screen<Reply>,
}

impl<Reply: TermReplySender + Clone> Parser<Reply> {
  /// Creates a new terminal parser of the given size and with the given
  /// amount of scrollback.
  #[must_use]
  pub fn new(
    rows: u16,
    cols: u16,
    scrollback_len: usize,
    reply_sender: Reply,
  ) -> Self {
    Self {
      screen: crate::vt100::screen::Screen::new(
        crate::vt100::grid::Size { rows, cols },
        scrollback_len,
        reply_sender,
      ),
    }
  }

  /// Resizes the terminal.
  pub fn set_size(&mut self, rows: u16, cols: u16) {
    self.screen.set_size(rows, cols);
  }

  /// Scrolls to the given position in the scrollback.
  ///
  /// This position indicates the offset from the top of the screen, and
  /// should be `0` to put the normal screen in view.
  ///
  /// This affects the return values of methods called on `parser.screen()`:
  /// for instance, `parser.screen().cell(0, 0)` will return the top left
  /// corner of the screen after taking the scrollback offset into account.
  /// It does not affect `parser.process()` at all.
  ///
  /// The value given will be clamped to the actual size of the scrollback.
  pub fn set_scrollback(&mut self, rows: usize) {
    self.screen.set_scrollback(rows);
  }

  /// Returns a reference to a `Screen` object containing the terminal
  /// state.
  #[must_use]
  pub fn screen(&self) -> &crate::vt100::screen::Screen<Reply> {
    &self.screen
  }
}
