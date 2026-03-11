use crate::{key::Key, mouse::MouseEvent};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum InternalTermEvent {
  Key(Key),
  Mouse(MouseEvent),
  Resize(u16, u16),
  FocusGained,
  FocusLost,
  CursorPos(u16, u16),
  PrimaryDeviceAttributes,

  ReplyKittyKeyboard(u8),
}

#[derive(Debug)]
pub enum KeyboardMode {
  Unknown,
  ModifyOtherKeys,
  Kitty,
  Win32,
}
