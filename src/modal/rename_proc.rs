use crossterm::event::{Event, KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;
use tui::{
  prelude::{Margin, Rect},
  text::Span,
  Frame,
};
use tui_input::Input;

use crate::{
  app::LoopAction, error::ResultLogger, event::AppEvent, state::State,
  theme::Theme, widgets::text_input::TextInput,
};

use super::modal::Modal;

pub struct RenameProcModal {
  input: Input,
  app_sender: UnboundedSender<AppEvent>,
}

impl RenameProcModal {
  pub fn new(app_sender: UnboundedSender<AppEvent>) -> Self {
    RenameProcModal {
      input: Input::default(),
      app_sender,
    }
  }
}

impl Modal for RenameProcModal {
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
        code: KeyCode::Enter,
        modifiers,
        ..
      }) if modifiers.is_empty() => {
        self
          .app_sender
          .send(AppEvent::CloseCurrentModal)
          .log_ignore();
        self
          .app_sender
          .send(AppEvent::RenameProc {
            name: self.input.value().to_string(),
          })
          .log_ignore();
        // Skip because RenameProc event will immediately rerender.
        return true;
      }
      Event::Key(KeyEvent {
        code: KeyCode::Esc,
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

    let req = tui_input::backend::crossterm::to_input_request(&event);
    if let Some(req) = req {
      self.input.handle(req);
      loop_action.render();
      return true;
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
    (42, 3)
  }

  fn render(&mut self, frame: &mut Frame) {
    let area = self.area(frame.size());
    let theme = Theme::default();

    let block = theme
      .pane(true)
      .title(Span::styled("Rename process", theme.pane_title(true)));
    frame.render_widget(block, area);

    let inner = area.inner(&Margin::new(1, 1));

    let mut cursor = (0u16, 0u16);
    let text_input = TextInput::new(&mut self.input);
    frame.render_stateful_widget(
      text_input,
      Rect::new(inner.x, inner.y, inner.width, 1),
      &mut cursor,
    );

    frame.set_cursor(cursor.0, cursor.1);
  }
}
