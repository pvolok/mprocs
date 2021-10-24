use tui::{buffer::Buffer, layout::Rect, widgets::Widget};

pub struct RenderWidget<F>
where
  F: Fn(&mut Buffer),
{
  painter: F,
}

impl<F> RenderWidget<F>
where
  F: Fn(&mut Buffer),
{
  pub fn new(f: F) -> RenderWidget<F> {
    RenderWidget { painter: f }
  }
}

impl<F> Widget for RenderWidget<F>
where
  F: Fn(&mut Buffer),
{
  fn render(self, _area: Rect, buf: &mut Buffer) {
    (self.painter)(buf)
  }
}
