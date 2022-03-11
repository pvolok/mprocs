use crossterm::event::Event;

pub enum AppEvent {
  TermRender,
  Key(Event),
}
