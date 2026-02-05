use crate::{
  app::LoopAction,
  event::AppEvent,
  kernel::{kernel_message::ProcContext, proc::ProcId},
  key::{Key, KeyCode},
  state::State,
  term::TermEvent,
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
    event: &TermEvent,
  ) -> bool {
    match event {
      TermEvent::Key(Key {
        code: KeyCode::Char('y'),
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        self
          .pc
          .send_self_custom(AppEvent::RemoveProc { id: self.id });
        // Skip because RemoveProc event will immediately rerender.
        return true;
      }
      TermEvent::Key(Key {
        code: KeyCode::Esc,
        mods,
        ..
      })
      | TermEvent::Key(Key {
        code: KeyCode::Char('n'),
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        loop_action.render();
        return true;
      }
      _ => (),
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
