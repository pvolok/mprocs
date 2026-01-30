use crossterm::event::{Event, KeyCode, KeyEvent};

use crate::{
  app::LoopAction,
  event::AppEvent,
  kernel::kernel_message::ProcContext,
  state::State,
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
    event: &Event,
  ) -> bool {
    match event {
      Event::Key(KeyEvent {
        code: KeyCode::Char('e'),
        modifiers,
        ..
      }) if modifiers.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        self.pc.send_self_custom(AppEvent::Quit);
        return true;
      }
      Event::Key(KeyEvent {
        code: KeyCode::Char('d'),
        modifiers,
        ..
      }) if modifiers.is_empty() => {
        if let Some(client_id) = state.current_client_id {
          self.pc.send_self_custom(AppEvent::CloseCurrentModal);
          self.pc.send_self_custom(AppEvent::Detach { client_id });
        }
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
