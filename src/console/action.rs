#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConsoleAction {
  FocusLeft,
  FocusDown,
  FocusUp,
  FocusRight,
  SelectNext,
  SelectPrev,
  Quit,
}
