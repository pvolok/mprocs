use std::io;

use tui::{
  backend::CrosstermBackend, layout::Rect, text::Text, widgets::Paragraph,
  Frame,
};

use crate::theme::Theme;

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_zoom_tip(area: Rect, frame: &mut Frame<Backend>) {
  let theme = Theme::default();

  let line = Text::from(" To exit zoom mode press <C-a>");
  let p = Paragraph::new(line).style(theme.zoom_tip());
  frame.render_widget(p, area);
}
