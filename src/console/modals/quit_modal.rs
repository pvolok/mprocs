use crate::console::state::ConsoleState;
use crate::term::Grid;
use crate::term::grid::Rect;
use crate::term::key::{Key, KeyCode, KeyMods};

use super::{Modal, ModalAction, ModalChoice, draw_choices};

const CHOICES: &[ModalChoice] = &[
  ModalChoice {
    key: 'q',
    label: "stop all",
  },
  ModalChoice {
    key: 'd',
    label: "detach",
  },
];

pub struct QuitModal;

impl Modal for QuitModal {
  fn title(&self) -> &str {
    "Quit"
  }

  fn size(&self) -> (u16, u16) {
    (22, 6)
  }

  fn draw_content(&self, grid: &mut Grid, area: Rect) {
    draw_choices(grid, area, CHOICES);
  }

  fn handle_key(&mut self, key: Key, state: &mut ConsoleState) -> ModalAction {
    match key.code {
      KeyCode::Char('d') if key.mods == KeyMods::NONE => {
        state.quit_modal = false;
        ModalAction::Detach
      }
      KeyCode::Char('q') if key.mods == KeyMods::NONE => {
        state.quit_modal = false;
        ModalAction::Quit
      }
      KeyCode::Esc => {
        state.quit_modal = false;
        ModalAction::None
      }
      _ => ModalAction::None,
    }
  }
}
