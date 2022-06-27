use std::io;

use tui::{
  backend::CrosstermBackend,
  layout::{Margin, Rect},
  style::{Color, Style},
  text::{Span, Spans, Text},
  widgets::{Clear, Paragraph},
  Frame,
};

use crate::{
  encode_term::print_key,
  event::AppEvent,
  keymap::Keymap,
  state::{Scope, State},
  theme::Theme,
};

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_keymap(
  area: Rect,
  frame: &mut Frame<Backend>,
  state: &mut State,
  keymap: &Keymap,
) {
  let theme = Theme::default();

  let block = theme
    .pane(false)
    .title(Span::styled("Help", theme.pane_title(false)));
  frame.render_widget(Clear, area);
  frame.render_widget(block, area);

  let items = match state.scope {
    Scope::Procs => vec![
      AppEvent::ToggleFocus,
      AppEvent::Quit,
      AppEvent::NextProc,
      AppEvent::PrevProc,
      AppEvent::StartProc,
      AppEvent::TermProc,
      AppEvent::RestartProc,
    ],
    Scope::Term => {
      vec![AppEvent::ToggleFocus]
    }
    Scope::TermZoom => Vec::new(),
  };
  let line = items
    .into_iter()
    .filter_map(|event| Some((keymap.resolve_key(state.scope, &event)?, event)))
    .map(|(key, event)| {
      vec![
        Span::raw(" <"),
        Span::styled(print_key(key), Style::default().fg(Color::Yellow)),
        Span::raw(": "),
        Span::raw(event.desc()),
        Span::raw("> "),
      ]
    })
    .flatten()
    .collect::<Vec<_>>();

  let line = Spans::from(line);
  let line = Text::from(vec![line]);

  let p = Paragraph::new(line);
  frame.render_widget(
    p,
    area.inner(&Margin {
      vertical: 1,
      horizontal: 1,
    }),
  );
}
