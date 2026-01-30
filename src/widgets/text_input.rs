use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_input::{Input, InputRequest};

use crate::vt100::{attrs::Attrs, grid::Rect, Grid};

pub fn render_text_input(
  input: &mut Input,
  area: Rect,
  grid: &mut Grid,
  cursor_pos: &mut (u16, u16),
) {
  let value = input.value();

  let left_trim = input.cursor().saturating_sub(area.width as usize);
  let (value, cursor) = if left_trim > 0 {
    let start =
      unicode_segmentation::UnicodeSegmentation::grapheme_indices(value, true)
        .nth(left_trim)
        .map_or_else(|| value.len(), |(len, _)| len);
    (&value[start..], input.cursor() - left_trim)
  } else {
    (value, input.cursor())
  };

  grid.fill_area(area, ' ', Attrs::default());
  grid.draw_text(area, value, Attrs::default());

  *cursor_pos = (area.x + cursor as u16, area.y);
}

pub fn to_input_request(evt: &Event) -> Option<InputRequest> {
  use InputRequest::*;
  use KeyCode::*;
  match evt {
    Event::Key(KeyEvent {
      code,
      modifiers,
      kind,
      state: _,
    }) if *kind == KeyEventKind::Press || *kind == KeyEventKind::Repeat => {
      match (*code, *modifiers) {
        (Backspace, KeyModifiers::NONE)
        | (Char('h'), KeyModifiers::CONTROL) => Some(DeletePrevChar),
        (Delete, KeyModifiers::NONE) => Some(DeleteNextChar),
        (Tab, KeyModifiers::NONE) => None,
        (Left, KeyModifiers::NONE) | (Char('b'), KeyModifiers::CONTROL) => {
          Some(GoToPrevChar)
        }
        (Left, KeyModifiers::CONTROL) | (Char('b'), KeyModifiers::META) => {
          Some(GoToPrevWord)
        }
        (Right, KeyModifiers::NONE) | (Char('f'), KeyModifiers::CONTROL) => {
          Some(GoToNextChar)
        }
        (Right, KeyModifiers::CONTROL) | (Char('f'), KeyModifiers::META) => {
          Some(GoToNextWord)
        }
        (Char('u'), KeyModifiers::CONTROL) => Some(DeleteLine),

        (Char('w'), KeyModifiers::CONTROL)
        | (Char('d'), KeyModifiers::META)
        | (Backspace, KeyModifiers::META)
        | (Backspace, KeyModifiers::ALT) => Some(DeletePrevWord),

        (Delete, KeyModifiers::CONTROL) => Some(DeleteNextWord),
        (Char('k'), KeyModifiers::CONTROL) => Some(DeleteTillEnd),
        (Char('a'), KeyModifiers::CONTROL) | (Home, KeyModifiers::NONE) => {
          Some(GoToStart)
        }
        (Char('e'), KeyModifiers::CONTROL) | (End, KeyModifiers::NONE) => {
          Some(GoToEnd)
        }
        (Char(c), KeyModifiers::NONE) => Some(InsertChar(c)),
        (Char(c), KeyModifiers::SHIFT) => Some(InsertChar(c)),
        (_, _) => None,
      }
    }
    _ => None,
  }
}
