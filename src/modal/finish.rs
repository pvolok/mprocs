use crossterm::event::{Event, KeyCode, KeyEvent};
use tui::{
  prelude::{Margin, Rect},
  text::Line,
  widgets::Paragraph,
  Frame,
};

use crate::{
  app::LoopAction, event::AppEvent, kernel::kernel_message::ProcContext,
  state::State, theme::Theme,
};

use super::modal::Modal;

pub struct FinishModal {
  pc: ProcContext,
}

impl FinishModal {
  pub fn new(pc: ProcContext) -> Self {
    FinishModal { pc }
  }
}

impl Modal for FinishModal {
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
        code: KeyCode::Char('y') | KeyCode::Char('q') | KeyCode::Enter,
        modifiers,
        ..
      }) if modifiers.is_empty() => {
        self.pc.send_self_custom(AppEvent::CloseCurrentModal);
        self.pc.send_self_custom(AppEvent::Quit);
        return true;
      }
      Event::Key(KeyEvent {
        code: KeyCode::Esc | KeyCode::Char('n'),
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
    (40, 5)
  }

  fn render(&mut self, frame: &mut Frame) {
    let area = self.area(frame.area());
    let theme = Theme::default();

    let block = theme.pane(true);
    frame.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 1));

    let text = vec![
      Line::from("All processes are finished."),
      Line::from("Quit? (y/n)"),
    ];
    let p = Paragraph::new(text);
    frame.render_widget(p, inner);
  }
}
