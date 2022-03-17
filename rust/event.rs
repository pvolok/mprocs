use crossterm::event::KeyEvent;

pub enum AppEvent {
  Quit,

  ToggleScope,

  NextProc,
  PrevProc,

  SendKey(KeyEvent),
}
