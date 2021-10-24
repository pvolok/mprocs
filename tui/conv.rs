use crate::ctypes::types::{
  Color, ColorOpt, Event, KeyCode, KeyEvent, KeyMods, MouseButton, MouseEvent,
  MouseEventKind, Style,
};

//
// Style
//

fn opt_color_of_c(x: ColorOpt) -> Option<tui::style::Color> {
  match x {
    ColorOpt::Some(x) => Some(Color::into(x)),
    ColorOpt::None => None,
  }
}

fn opt_color_to_c(x: Option<tui::style::Color>) -> ColorOpt {
  match x {
    Some(x) => ColorOpt::Some(Color::from(x)),
    None => ColorOpt::None,
  }
}

pub fn style_of_c(x: Style) -> tui::style::Style {
  tui::style::Style {
    fg: opt_color_of_c(x.fg),
    bg: opt_color_of_c(x.bg),
    add_modifier: tui::style::Modifier::from_bits_truncate(x.add_modifier),
    sub_modifier: tui::style::Modifier::from_bits_truncate(x.sub_modifier),
  }
}

pub fn style_to_c(x: tui::style::Style) -> Style {
  Style {
    fg: opt_color_to_c(x.fg),
    bg: opt_color_to_c(x.bg),
    add_modifier: x.add_modifier.bits(),
    sub_modifier: x.sub_modifier.bits(),
  }
}

//
// Event
//

fn key_mods_to_c(mods: crossterm::event::KeyModifiers) -> KeyMods {
  KeyMods {
    shift: mods.intersects(crossterm::event::KeyModifiers::SHIFT) as u8,
    control: mods.intersects(crossterm::event::KeyModifiers::CONTROL) as u8,
    alt: mods.intersects(crossterm::event::KeyModifiers::ALT) as u8,
  }
}

fn mouse_button_to_c(button: crossterm::event::MouseButton) -> MouseButton {
  match button {
    crossterm::event::MouseButton::Left => MouseButton::Left,
    crossterm::event::MouseButton::Right => MouseButton::Right,
    crossterm::event::MouseButton::Middle => MouseButton::Middle,
  }
}

fn mouse_event_kind_to_c(
  kind: crossterm::event::MouseEventKind,
) -> MouseEventKind {
  match kind {
    crossterm::event::MouseEventKind::Down(btn) => {
      MouseEventKind::Down(mouse_button_to_c(btn))
    }
    crossterm::event::MouseEventKind::Up(btn) => {
      MouseEventKind::Up(mouse_button_to_c(btn))
    }
    crossterm::event::MouseEventKind::Drag(btn) => {
      MouseEventKind::Drag(mouse_button_to_c(btn))
    }
    crossterm::event::MouseEventKind::Moved => MouseEventKind::Moved,
    crossterm::event::MouseEventKind::ScrollDown => MouseEventKind::ScrollDown,
    crossterm::event::MouseEventKind::ScrollUp => MouseEventKind::ScrollUp,
  }
}

pub fn event_to_c(c_event: crossterm::event::Event) -> Event {
  match c_event {
    crossterm::event::Event::Key(key_event) => {
      let key_event = KeyEvent {
        code: match key_event.code {
          crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
          crossterm::event::KeyCode::Enter => KeyCode::Enter,
          crossterm::event::KeyCode::Left => KeyCode::Left,
          crossterm::event::KeyCode::Right => KeyCode::Right,
          crossterm::event::KeyCode::Up => KeyCode::Up,
          crossterm::event::KeyCode::Down => KeyCode::Down,
          crossterm::event::KeyCode::Home => KeyCode::Home,
          crossterm::event::KeyCode::End => KeyCode::End,
          crossterm::event::KeyCode::PageUp => KeyCode::PageUp,
          crossterm::event::KeyCode::PageDown => KeyCode::PageDown,
          crossterm::event::KeyCode::Tab => KeyCode::Tab,
          crossterm::event::KeyCode::BackTab => KeyCode::BackTab,
          crossterm::event::KeyCode::Delete => KeyCode::Delete,
          crossterm::event::KeyCode::Insert => KeyCode::Insert,
          crossterm::event::KeyCode::F(x) => KeyCode::F(x),
          crossterm::event::KeyCode::Char(c) => KeyCode::Char(c as u32),
          crossterm::event::KeyCode::Null => KeyCode::Null,
          crossterm::event::KeyCode::Esc => KeyCode::Esc,
        },
        modifiers: key_mods_to_c(key_event.modifiers),
      };
      Event::Key(key_event)
    }
    crossterm::event::Event::Mouse(mouse_event) => {
      let mouse_event = MouseEvent {
        kind: mouse_event_kind_to_c(mouse_event.kind),
        column: mouse_event.column,
        row: mouse_event.row,
        modifiers: key_mods_to_c(mouse_event.modifiers),
      };
      Event::Mouse(mouse_event)
    }
    crossterm::event::Event::Resize(w, h) => Event::Resize(w, h),
  }
}
