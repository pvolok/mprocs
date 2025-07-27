use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;
use tui::{
  prelude::{Margin, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Clear, HighlightSpacing, ListItem, ListState, Paragraph},
  Frame,
};
use tui_input::Input;

use crate::{
  app::LoopAction, error::ResultLogger, event::AppEvent, state::State,
  theme::Theme, widgets::text_input::TextInput,
};

use super::modal::Modal;

pub struct CommandsMenuModal {
  input: Input,
  list_state: ListState,
  items: Vec<CommandInfo>,
  app_sender: UnboundedSender<AppEvent>,
}

impl CommandsMenuModal {
  pub fn new(app_sender: UnboundedSender<AppEvent>) -> Self {
    CommandsMenuModal {
      input: Input::default(),
      list_state: ListState::default().with_selected(Some(0)),
      items: get_commands(""),
      app_sender,
    }
  }
}

impl Modal for CommandsMenuModal {
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
        if let Some((_, _, event)) =
          self.list_state.selected().and_then(|i| self.items.get(i))
        {
          self.app_sender.send(event.clone()).unwrap();
        }
        // Skip because AddProc event will immediately rerender.
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
      // List bindings
      Event::Key(KeyEvent {
        code: KeyCode::Char('n'),
        modifiers,
        ..
      }) if modifiers == &KeyModifiers::CONTROL => {
        let index = self.list_state.selected().unwrap_or(0);
        let index = if index >= self.items.len() - 1 {
          0
        } else {
          index + 1
        };
        self.list_state.select(Some(index));
        loop_action.render();
        return true;
      }
      Event::Key(KeyEvent {
        code: KeyCode::Char('p'),
        modifiers,
        ..
      }) if modifiers == &KeyModifiers::CONTROL => {
        let index = self.list_state.selected().unwrap_or(0);
        let index = if index == 0 {
          self.items.len() - 1
        } else {
          index - 1
        };
        self.list_state.select(Some(index));
        loop_action.render();
        return true;
      }
      _ => (),
    }

    let req = tui_input::backend::crossterm::to_input_request(event);
    if let Some(req) = req {
      let res = self.input.handle(req);
      if let Some(res) = res {
        if res.value {
          self.items = get_commands(self.input.value());
        }
      }
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
    (60, 30)
  }

  fn render(&mut self, frame: &mut Frame) {
    let area = self.area(frame.size());
    let theme = Theme::default();

    let block = theme
      .pane(true)
      .border_type(tui::widgets::BorderType::Rounded);
    frame.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 1));
    let list_area = Rect::new(
      inner.x,
      inner.y,
      inner.width,
      inner.height.saturating_sub(2),
    );
    let above_input = Rect::new(
      inner.x,
      (inner.y + inner.height).saturating_sub(2),
      inner.width,
      1,
    );

    frame.render_widget(Clear, inner);

    let list_items = self
      .items
      .iter()
      .map(|(cmd, desc, _event)| {
        let line = Line::from(vec![
          Span::styled(*cmd, Style::reset().fg(tui::style::Color::White)),
          "  ".into(),
          Span::styled(
            desc,
            Style::reset()
              .fg(tui::style::Color::DarkGray)
              .add_modifier(Modifier::ITALIC),
          ),
        ]);
        ListItem::new(line)
      })
      .collect::<Vec<_>>();
    let list = tui::widgets::List::new(list_items)
      .highlight_spacing(HighlightSpacing::Always)
      .highlight_symbol(">")
      .direction(tui::widgets::ListDirection::TopToBottom);
    frame.render_stateful_widget(list, list_area, &mut self.list_state);

    let input_label = "Run command";
    frame.render_widget(Paragraph::new(input_label), above_input);

    frame.render_widget(
      Paragraph::new(tui::symbols::line::VERTICAL_RIGHT),
      Rect::new(area.x, above_input.y, 1, 1),
    );
    frame.render_widget(
      Paragraph::new(tui::symbols::line::VERTICAL_LEFT),
      Rect::new(above_input.right(), above_input.y, 1, 1),
    );
    for x in above_input.x + input_label.len() as u16
      ..above_input.x + above_input.width
    {
      frame.render_widget(
        Paragraph::new(tui::symbols::line::HORIZONTAL),
        Rect::new(x, above_input.y, 1, 1),
      );
    }

    let mut cursor = (0u16, 0u16);
    let text_input = TextInput::new(&mut self.input);
    frame.render_stateful_widget(
      text_input,
      Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
      &mut cursor,
    );

    frame.set_cursor(cursor.0, cursor.1);
  }
}

type CommandInfo = (&'static str, String, AppEvent);

fn get_commands(search: &str) -> Vec<CommandInfo> {
  let events = [
    // ("quit-or-ask", AppEvent::QuitOrAsk),
    ("quit", AppEvent::Quit),
    ("force-quit", AppEvent::ForceQuit),
    ("toggle-focus", AppEvent::ToggleFocus),
    ("focus-term", AppEvent::FocusTerm),
    ("zoom", AppEvent::Zoom),
    ("show-commands-menu", AppEvent::ShowCommandsMenu),
    ("next-proc", AppEvent::NextProc),
    ("prev-proc", AppEvent::PrevProc),
    ("start-proc", AppEvent::StartProc),
    ("term-proc", AppEvent::TermProc),
    ("kill-proc", AppEvent::KillProc),
    ("restart-proc", AppEvent::RestartProc),
    ("duplicate-proc", AppEvent::DuplicateProc),
    ("force-restart-proc", AppEvent::ForceRestartProc),
    ("show-add-proc", AppEvent::ShowAddProc),
    ("show-rename-proc", AppEvent::ShowRenameProc),
    ("show-remove-proc", AppEvent::ShowRemoveProc),
    ("close-current-modal", AppEvent::CloseCurrentModal),
    ("scroll-down", AppEvent::ScrollDown),
    ("scroll-up", AppEvent::ScrollUp),
    ("copy-mode-enter", AppEvent::CopyModeEnter),
    ("copy-mode-leave", AppEvent::CopyModeLeave),
    ("copy-mode-end", AppEvent::CopyModeEnd),
    ("copy-mode-copy", AppEvent::CopyModeCopy),
  ];

  let mut result = Vec::new();
  for (cmd, event) in events {
    let desc = event.desc();
    if cmd.contains(search) || desc.contains(search) {
      result.push((cmd, desc, event));
    }
  }

  result
}
