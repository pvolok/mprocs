use crossterm::event::{Event, KeyCode, KeyEvent};

use crate::{
  app::LoopAction,
  event::AppEvent,
  kernel::{kernel_message::ProcContext, proc::ProcId},
  state::State,
  vt100::{
    attrs::Attrs,
    grid::{BorderType, Rect},
    Grid,
  },
};

use super::modal::Modal;

pub struct RemoveProcModal {
  pc: ProcContext,
  id: ProcId,
}

impl RemoveProcModal {
  pub fn new(id: ProcId, pc: ProcContext) -> Self {
    RemoveProcModal { pc, id }
  }
}

impl Modal for RemoveProcModal {
  fn handle_input(
    &mut self,
    _state: &mut State,
    loop_action: &mut LoopAction,
    event: &Event,
  ) -> bool {
    match event {
      Event::Key(KeyEvent {
        code: KeyCode::Char('y'),
        modifiers,
        ..
      }) if modifiers.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        self
          .pc
          .send_self_custom(AppEvent::RemoveProc { id: self.id });
        // Skip because RemoveProc event will immediately rerender.
        return true;
      }
      Event::Key(KeyEvent {
        code: KeyCode::Esc,
        modifiers,
        ..
      })
      | Event::Key(KeyEvent {
        code: KeyCode::Char('n'),
        modifiers,
        ..
      }) if modifiers.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        loop_action.render();
        return true;
      }
      _ => (),
    }

    match event {
      Event::FocusGained => false,
      Event::FocusLost => false,
      // Block keys
      Event::Key(_) => true,
      // Block mouse
      Event::Mouse(_) => true,
      // Block paste
      Event::Paste(_) => true,
      Event::Resize(_, _) => false,
    }
  }

  fn get_size(&mut self, _: Rect) -> (u16, u16) {
    (36, 3)
  }

  fn render(&mut self, grid: &mut Grid) {
    let area = self.area(grid.area());

    grid.draw_block(area, BorderType::Thick, Attrs::default());

    let inner = area.inner(1);

    let txt_area = Rect {
      x: inner.x,
      y: inner.y,
      width: inner.width,
      height: 1,
    };
    grid.fill_area(txt_area, ' ', Attrs::default());
    grid.draw_text(txt_area, "Remove process? (y/n)", Attrs::default());
  }
}
