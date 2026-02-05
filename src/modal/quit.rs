use crate::{
  app::LoopAction,
  event::AppEvent,
  kernel::kernel_message::ProcContext,
  key::{Key, KeyCode},
  state::State,
  term::TermEvent,
  vt100::{
    attrs::Attrs,
    grid::{BorderType, Rect},
  },
};

use super::modal::Modal;

pub struct QuitModal {
  pc: ProcContext,
}

impl QuitModal {
  pub fn new(pc: ProcContext) -> Self {
    QuitModal { pc }
  }
}

impl Modal for QuitModal {
  fn handle_input(
    &mut self,
    state: &mut State,
    loop_action: &mut LoopAction,
    event: &TermEvent,
  ) -> bool {
    match event {
      TermEvent::Key(Key {
        code: KeyCode::Char('e'),
        mods,
        ..
      }) if mods.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        self.pc.send_self_custom(AppEvent::Quit);
        return true;
      }
      TermEvent::Key(Key {
        code: KeyCode::Char('d'),
        mods,
        ..
      }) if mods.is_empty() => {
        if let Some(client_id) = state.current_client_id {
          self.pc.send_self_custom(AppEvent::CloseCurrentModal);
          self.pc.send_self_custom(AppEvent::Detach { client_id });
        }
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
    (36, 5)
  }

  fn render(&mut self, grid: &mut crate::vt100::Grid) {
    use crate::vt100::grid::Rect;

    let area = self.area(grid.area());

    grid.draw_block(area, BorderType::Thick, Attrs::default());

    let inner = area.inner(1);

    let txt_area = Rect {
      x: inner.x,
      y: inner.y,
      width: inner.width,
      height: 1,
    };
    grid.fill_area(inner, ' ', Attrs::default());
    grid.draw_text(
      Rect {
        y: inner.y,
        ..txt_area
      },
      "<e>   - exit client and server",
      Attrs::default(),
    );
    grid.draw_text(
      Rect {
        y: inner.y + 1,
        ..txt_area
      },
      "<d>   - detach client",
      Attrs::default(),
    );
    grid.draw_text(
      Rect {
        y: inner.y + 2,
        ..txt_area
      },
      "<Esc> - cancel",
      Attrs::default(),
    );
  }
}
