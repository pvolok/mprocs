use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Debug)]
pub enum InternalTermEvent {
  Key(KeyEvent),
  Mouse(MouseEvent),
  Resize(u16, u16),

  InitTimeout,
  ReplyKittyKeyboard(u8),
}

#[derive(Debug)]
pub enum KeyboardMode {
  Unknown,
  ModifyOtherKeys,
  Kitty(u8),
}
