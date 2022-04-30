use crossterm::event::KeyEvent;

pub enum AppEvent {
  Quit,
  ForceQuit,

  ToggleScope,

  NextProc,
  PrevProc,
  StartProc,
  TermProc,
  KillProc,

  ScrollDown,
  ScrollUp,

  SendKey(KeyEvent),
}
