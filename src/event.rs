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
  RestartProc,
  ForceRestartProc,

  ScrollDown,
  ScrollUp,

  SendKey(KeyEvent),
}
