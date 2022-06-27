use std::io;

use tui::{
  backend::CrosstermBackend, layout::Rect, text::Text, widgets::Paragraph,
  Frame,
};

use crate::{event::AppEvent, keymap::Keymap, state::Scope, theme::Theme};

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_zoom_tip(
  area: Rect,
  frame: &mut Frame<Backend>,
  keymap: &Keymap,
) {
  let theme = Theme::default();

  let events = vec![
    AppEvent::FocusTerm,
    AppEvent::ToggleFocus,
    AppEvent::FocusProcs,
  ];
  let key = events
    .into_iter()
    .find_map(|event| keymap.resolve_key(Scope::TermZoom, &event));

  let line = if let Some(key) = key {
    Text::from(format!(" To exit zoom mode press {}", key.to_string()))
  } else {
    Text::from(" No key bound to exit the zoom mode")
  };
  let p = Paragraph::new(line).style(theme.zoom_tip());
  frame.render_widget(p, area);
}
