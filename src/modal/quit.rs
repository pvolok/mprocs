use crossterm::event::{Event, KeyCode, KeyEvent};
use tui::{
  prelude::{Margin, Rect},
  text::Line,
  widgets::{Clear, Paragraph},
  Frame,
};

use crate::{
  app::LoopAction, event::AppEvent, kernel2::kernel_message::ProcContext,
  state::State, theme::Theme,
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
  fn boxed(self) -> Box<dyn Modal> {
    Box::new(self)
  }

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

  fn render(&mut self, frame: &mut Frame) {
    let area = self.area(frame.area());
    let theme = Theme::default();

    let block = theme.pane(true);
    frame.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 1));

    let txt = Paragraph::new(vec![
      Line::from("<e>   - exit client and server"),
      Line::from("<d>   - detach client"),
      Line::from("<Esc> - cancel"),
    ]);
    let txt_area = Rect::new(inner.x, inner.y, inner.width, 3);
    frame.render_widget(Clear, txt_area);
    frame.render_widget(txt, txt_area);
  }
}
