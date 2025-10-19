use crossterm::event::KeyEvent;

#[derive(Debug)]
pub enum InternalTermEvent {
  Key(KeyEvent),
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
