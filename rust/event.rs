use crossterm::event::KeyEvent;

pub enum AppEvent {
  Quit,

  ToggleScope,

  NextProc,
  PrevProc,
  StartProc,
  TermProc,
  KillProc,

  SendKey(KeyEvent),
}
