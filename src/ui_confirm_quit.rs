use tui::{
  layout::Rect,
  widgets::{Clear, Paragraph},
  Frame,
};

use crate::{protocol::ProxyBackend, theme::Theme};

type Backend = ProxyBackend;

pub fn render_confirm_quit(area: Rect, frame: &mut Frame<Backend>) {
  let theme = Theme::default();

  let y = area.height / 2;
  let x = (area.width / 2).saturating_sub(20).max(1);

  let block = theme.pane(true);
  frame.render_widget(block, Rect::new(x - 1, y - 1, 42, 3).intersection(area));

  let txt = Paragraph::new("Stop processes and quit? (y/n)");
  let txt_area = Rect::new(x, y, 40, 1).intersection(area);
  frame.render_widget(Clear, txt_area);
  frame.render_widget(txt, txt_area);
}
