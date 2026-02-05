use tui_input::Input;

use crate::{
  app::LoopAction,
  event::AppEvent,
  kernel::kernel_message::ProcContext,
  key::{Key, KeyCode},
  state::State,
  term::TermEvent,
  vt100::{
    attrs::Attrs,
    grid::{Pos, Rect},
    Grid,
  },
  widgets::text_input::{render_text_input, to_input_request},
};

use super::modal::Modal;

pub struct AddProcModal {
  pc: ProcContext,
  input: Input,
}

impl AddProcModal {
  pub fn new(pc: ProcContext) -> Self {
    AddProcModal {
      pc,
      input: Input::default(),
    }
  }
}

impl Modal for AddProcModal {
  fn handle_input(
    &mut self,
    _state: &mut State,
    loop_action: &mut LoopAction,
    event: &TermEvent,
  ) -> bool {
    match event {
      TermEvent::Key(Key {
        code: KeyCode::Enter,
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        self.pc.send_self_custom(AppEvent::AddProc {
          cmd: self.input.value().to_string(),
          name: None,
        });
        // Skip because AddProc event will immediately rerender.
        return true;
      }
      TermEvent::Key(Key {
        code: KeyCode::Esc,
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        loop_action.render();
        return true;
      }
      _ => (),
    }

    let req = to_input_request(event);
    if let Some(req) = req {
      self.input.handle(req);
      loop_action.render();
      return true;
    }

    match event {
      TermEvent::FocusGained => false,
      TermEvent::FocusLost => false,
      // Block keys
      TermEvent::Key(_) => true,
      // Block mouse
      TermEvent::Mouse(_) => true,
      // Block paste
      TermEvent::Paste(_) => true,
      TermEvent::Resize(_, _) => false,
    }
  }

  fn get_size(&mut self, _: Rect) -> (u16, u16) {
    (42, 3)
  }

  fn render(&mut self, grid: &mut Grid) {
    let area = self.area(Rect {
      x: 0,
      y: 0,
      width: grid.size().width,
      height: grid.size().height,
    });
    grid.draw_block(
      area.into(),
      crate::vt100::grid::BorderType::Plain,
      Attrs::default(),
    );
    grid.draw_text(
      Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: 1,
      },
      "Add process",
      Attrs::default(),
    );

    let inner = Rect {
      x: area.x + 1,
      y: area.y + 1,
      width: area.width.saturating_sub(2),
      height: 1,
    };

    let mut cursor = (0u16, 0u16);
    render_text_input(&mut self.input, inner, grid, &mut cursor);

    grid.cursor_pos = Some(Pos {
      col: cursor.0,
      row: cursor.1,
    });
  }
}
