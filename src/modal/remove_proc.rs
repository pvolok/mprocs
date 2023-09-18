use crossterm::event::{Event, KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;
use tui::{
  prelude::{Margin, Rect},
  widgets::{Clear, Paragraph},
  Frame,
};

use crate::{
  app::LoopAction, error::ResultLogger, event::AppEvent,
  protocol::ProxyBackend, state::State, theme::Theme,
};

use super::modal::Modal;

pub struct RemoveProcModal {
  id: usize,
  app_sender: UnboundedSender<AppEvent>,
}

impl RemoveProcModal {
  pub fn new(id: usize, app_sender: UnboundedSender<AppEvent>) -> Self {
    RemoveProcModal { id, app_sender }
  }
}

impl Modal for RemoveProcModal {
  fn boxed(self) -> Box<dyn Modal> {
    Box::new(self)
  }

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
        self
          .app_sender
          .send(AppEvent::CloseCurrentModal)
          .log_ignore();
        self
          .app_sender
          .send(AppEvent::RemoveProc { id: self.id })
          .log_ignore();
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
        self
          .app_sender
          .send(AppEvent::CloseCurrentModal)
          .log_ignore();
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

  fn get_size(&mut self) -> (u16, u16) {
    (36, 3)
  }

  fn render(&mut self, frame: &mut Frame<ProxyBackend>) {
    let area = self.area(frame.size());
    let theme = Theme::default();

    let block = theme.pane(true);
    frame.render_widget(block, area);

    let inner = area.inner(&Margin::new(1, 1));

    let txt = Paragraph::new("Remove process? (y/n)");
    let txt_area = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(Clear, txt_area);
    frame.render_widget(txt, txt_area);
  }
}
