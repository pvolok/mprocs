use tui_input::{Input, InputRequest};

use crate::{
  key::{Key, KeyCode, KeyEventKind, KeyMods},
  term::TermEvent,
  vt100::{attrs::Attrs, grid::Rect, Grid},
};

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

pub fn to_input_request(evt: &TermEvent) -> Option<InputRequest> {
  use InputRequest::*;
  use KeyCode::*;
  match evt {
    TermEvent::Key(Key {
      code,
      mods,
      kind,
      state: _,
    }) if *kind == KeyEventKind::Press || *kind == KeyEventKind::Repeat => {
      match (*code, *mods) {
        (Backspace, KeyMods::NONE) | (Char('h'), KeyMods::CONTROL) => {
          Some(DeletePrevChar)
        }
        (Delete, KeyMods::NONE) => Some(DeleteNextChar),
        (Tab, KeyMods::NONE) => None,
        (Left, KeyMods::NONE) | (Char('b'), KeyMods::CONTROL) => {
          Some(GoToPrevChar)
        }
        (Left, KeyMods::CONTROL) | (Char('b'), KeyMods::META) => {
          Some(GoToPrevWord)
        }
        (Right, KeyMods::NONE) | (Char('f'), KeyMods::CONTROL) => {
          Some(GoToNextChar)
        }
        (Right, KeyMods::CONTROL) | (Char('f'), KeyMods::META) => {
          Some(GoToNextWord)
        }
        (Char('u'), KeyMods::CONTROL) => Some(DeleteLine),

        (Char('w'), KeyMods::CONTROL)
        | (Char('d'), KeyMods::META)
        | (Backspace, KeyMods::META)
        | (Backspace, KeyMods::ALT) => Some(DeletePrevWord),

        (Delete, KeyMods::CONTROL) => Some(DeleteNextWord),
        (Char('k'), KeyMods::CONTROL) => Some(DeleteTillEnd),
        (Char('a'), KeyMods::CONTROL) | (Home, KeyMods::NONE) => {
          Some(GoToStart)
        }
        (Char('e'), KeyMods::CONTROL) | (End, KeyMods::NONE) => Some(GoToEnd),
        (Char(c), KeyMods::NONE) => Some(InsertChar(c)),
        (Char(c), KeyMods::SHIFT) => Some(InsertChar(c)),
        (_, _) => None,
      }
    }
    _ => None,
  }
}
