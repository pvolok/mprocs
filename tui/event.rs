use crossterm::event as evt;

//
// Key event
//

#[derive(ocaml::IntoValue)]
pub enum KeyCode {
  Backspace,
  Enter,
  Left,
  Right,
  Up,
  Down,
  Home,
  End,
  PageUp,
  PageDown,
  Tab,
  BackTab,
  Delete,
  Insert,
  F(ocaml::Int),
  Char(ocaml::Int),
  Null,
  Esc,
}

#[derive(ocaml::IntoValue)]
pub struct KeyMods {
  pub shift: bool,
  pub control: bool,
  pub alt: bool,
}

#[derive(ocaml::IntoValue)]
pub struct KeyEvent {
  pub code: KeyCode,
  pub modifiers: KeyMods,
}

//
// Mouse event
//

#[derive(ocaml::IntoValue)]
pub enum MouseButton {
  Left,
  Right,
  Middle,
}

#[derive(ocaml::IntoValue)]
pub enum MouseEventKind {
  Down(MouseButton),
  Up(MouseButton),
  Drag(MouseButton),
  Moved,
  ScrollDown,
  ScrollUp,
}

#[derive(ocaml::IntoValue)]
pub struct MouseEvent {
  pub kind: MouseEventKind,
  pub column: u16,
  pub row: u16,
  pub modifiers: KeyMods,
}

//
// Event
//

#[derive(ocaml::IntoValue)]
pub enum Event {
  Key(KeyEvent),
  Mouse(MouseEvent),
  Resize(u16, u16),
}

fn conv_key_mods(mods: evt::KeyModifiers) -> KeyMods {
  KeyMods {
    shift: mods.intersects(evt::KeyModifiers::SHIFT),
    control: mods.intersects(evt::KeyModifiers::CONTROL),
    alt: mods.intersects(evt::KeyModifiers::ALT),
  }
}

pub fn from_crossterm(c_event: evt::Event) -> Event {
  match c_event {
    evt::Event::Key(key_event) => {
      let key_event = KeyEvent {
        code: match key_event.code {
          evt::KeyCode::Backspace => KeyCode::Backspace,
          evt::KeyCode::Enter => KeyCode::Enter,
          evt::KeyCode::Left => KeyCode::Left,
          evt::KeyCode::Right => KeyCode::Right,
          evt::KeyCode::Up => KeyCode::Up,
          evt::KeyCode::Down => KeyCode::Down,
          evt::KeyCode::Home => KeyCode::Home,
          evt::KeyCode::End => KeyCode::End,
          evt::KeyCode::PageUp => KeyCode::PageUp,
          evt::KeyCode::PageDown => KeyCode::PageDown,
          evt::KeyCode::Tab => KeyCode::Tab,
          evt::KeyCode::BackTab => KeyCode::BackTab,
          evt::KeyCode::Delete => KeyCode::Delete,
          evt::KeyCode::Insert => KeyCode::Insert,
          evt::KeyCode::F(x) => KeyCode::F(ocaml::Int::from(x)),
          evt::KeyCode::Char(c) => KeyCode::Char(ocaml::Int::from(c as isize)),
          evt::KeyCode::Null => KeyCode::Null,
          evt::KeyCode::Esc => KeyCode::Esc,
        },
        modifiers: conv_key_mods(key_event.modifiers),
      };
      Event::Key(key_event)
    }
    evt::Event::Mouse(_) => todo!(),
    evt::Event::Resize(w, h) => Event::Resize(w, h),
  }
}
