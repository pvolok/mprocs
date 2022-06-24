use std::{io, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
  key::Key,
  keymap::Keymap,
  settings::Settings,
  state::{Scope, State},
  theme::Theme,
};

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_keymap(
  area: Rect,
  frame: &mut Frame<Backend>,
  state: &mut State,
  keymap: Rc<Keymap>,
) {
  let theme = Theme::default();

  let block = theme
    .pane(false)
    .title(Span::styled("Help", theme.pane_title(false)));
  frame.render_widget(Clear, area);
  frame.render_widget(block, area);

  let items = match state.scope {
    Scope::Procs => {
      let settings = Settings::default();
      let prev_default = Key::new(KeyCode::Char('j'), KeyModifiers::NONE);
      let prev = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::PrevProc,
          settings.default_keys(Scope::Procs, &AppEvent::PrevProc),
        )
        .unwrap_or(&prev_default);
      let quit_default = Key::new(KeyCode::Char('q'), KeyModifiers::NONE);
      let quit = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::Quit,
          settings.default_keys(Scope::Procs, &AppEvent::Quit),
        )
        .unwrap_or(&quit_default);
      let toggle_default = Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
      let toggle = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::ToggleFocus,
          settings.default_keys(Scope::Procs, &AppEvent::ToggleFocus),
        )
        .unwrap_or(&toggle_default);
      let next_default = Key::new(KeyCode::Char('j'), KeyModifiers::NONE);
      let next = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::NextProc,
          settings.default_keys(Scope::Procs, &AppEvent::NextProc),
        )
        .unwrap_or(&next_default);
      let start_default = Key::new(KeyCode::Char('s'), KeyModifiers::NONE);
      let start = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::StartProc,
          settings.default_keys(Scope::Procs, &AppEvent::StartProc),
        )
        .unwrap_or(&start_default);
      let stop_default = Key::new(KeyCode::Char('x'), KeyModifiers::NONE);
      let stop = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::TermProc,
          settings.default_keys(Scope::Procs, &AppEvent::TermProc),
        )
        .unwrap_or(&stop_default);
      let restart_default = Key::new(KeyCode::Char('r'), KeyModifiers::NONE);
      let restart = keymap
        .non_default_key(
          Scope::Procs,
          &AppEvent::RestartProc,
          settings.default_keys(Scope::Procs, &AppEvent::RestartProc),
        )
        .unwrap_or(&restart_default);
      vec![
        (*toggle.code(), *toggle.mods(), "Toggle focus"),
        (*quit.code(), *quit.mods(), "Quit"),
        (*next.code(), *next.mods(), "Next"),
        (*prev.code(), *prev.mods(), "Prev"),
        (*start.code(), *start.mods(), "Start"),
        (*stop.code(), *stop.mods(), "Stop"),
        (*restart.code(), *restart.mods(), "Restart"),
      ]
    }
    Scope::Term => {
      let toggle_default = Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
      let toggle = keymap
        .non_default_key(
          Scope::Term,
          &AppEvent::ToggleFocus,
          Settings::default().default_keys(Scope::Term, &AppEvent::ToggleFocus),
        )
        .unwrap_or(&toggle_default);
      vec![(*toggle.code(), *toggle.mods(), "Toggle focus")]
    }
    Scope::TermZoom => Vec::new(),
  };
  let line = items
    .into_iter()
    .map(|(code, mods, desc)| (KeyEvent::new(code, mods), desc))
    .map(|(key, desc)| {
      vec![
        Span::raw(" <"),
        Span::styled(print_key(key), Style::default().fg(Color::Yellow)),
        Span::raw(": "),
        Span::raw(desc),
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
