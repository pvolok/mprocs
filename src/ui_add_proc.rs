use std::io;

use tui::{
  backend::CrosstermBackend,
  layout::Rect,
  style::{Color, Style},
  text::Span,
  widgets::{Block, BorderType, Borders, Clear, Paragraph},
  Frame,
};
use tui_input::Input;

use crate::theme::Theme;

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_add_proc(
  area: Rect,
  frame: &mut Frame<Backend>,
  input: &mut Input,
) {
  let theme = Theme::default();

  let y = area.height / 2;
  let x = (area.width / 2).saturating_sub(20).max(1);
  let w = 39.min(area.width.saturating_sub(3));

  let block = Block::default()
    .title(Span::styled("Add process", theme.pane_title(true)))
    .borders(Borders::ALL)
    .border_style(theme.pane_border(true))
    .border_type(BorderType::Rounded)
    .style(Style::default().bg(Color::Black));
  frame.render_widget(block, Rect::new(x - 1, y - 1, 42, 3).intersection(area));

  let left_trim = input.cursor().saturating_sub(w as usize);
  let value = input.value();
  let (value, cursor) = if left_trim > 0 {
    let start =
      unicode_segmentation::UnicodeSegmentation::grapheme_indices(value, true)
        .skip(left_trim)
        .next()
        .map_or_else(|| value.len(), |(len, _)| len);
    (&value[start..], input.cursor() - left_trim)
  } else {
    (value, input.cursor())
  };
  let txt = Paragraph::new(value);
  let txt_area = Rect::new(x, y, 40, 1).intersection(area);
  frame.render_widget(Clear, txt_area);
  frame.render_widget(txt, txt_area);

  frame.set_cursor(x + cursor as u16, y);
}
