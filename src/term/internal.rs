use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Clone, Debug)]
pub enum InternalTermEvent {
  Key(KeyEvent),
  Mouse(MouseEvent),
  Resize(u16, u16),
  FocusGained,
  FocusLost,
  CursorPos(u16, u16),
  PrimaryDeviceAttributes,

  InitTimeout,
  ReplyKittyKeyboard(u8),
}

#[derive(Debug)]
pub enum KeyboardMode {
  Unknown,
  ModifyOtherKeys,
  Kitty,
  Win32,
}
