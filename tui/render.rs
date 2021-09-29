use std::{cell::RefCell, io};

use ocaml::{Error, Value};
use tui::{
  backend::CrosstermBackend,
  style::Style,
  text::Span,
  widgets::{Block, Borders},
  Frame,
};

use crate::{
  layout::RectML, render_widget::RenderWidget, style::StyleML,
  terminal::use_term,
};

type Backend = CrosstermBackend<io::Stdout>;

#[ocaml::func]
pub fn tui_render(draw: ocaml::Value) -> Result<(), Error> {
  crate::terminal::use_term(|term| term.terminal.autoresize())?;

  let result = unsafe { draw.call(gc, Value::unit()).map(|_| ()) };

  crate::terminal::use_term(|term| -> Result<(), io::Error> {
    term.terminal.draw(|_f| ())?;
    Ok(())
  })?;

  result
}

fn with_frame<F, R>(f: F) -> R
where
  F: FnOnce(&mut Frame<Backend>) -> R,
{
  use_term(|term| {
    let mut frame = term.terminal.get_frame();
    f(&mut frame)
  })
}

#[ocaml::func]
pub fn tui_render_frame_size(_f: ocaml::Value) -> RectML {
  with_frame(|f| RectML::of_tui(f.size()))
}

#[ocaml::func]
pub fn tui_render_block(
  _f: ocaml::Value,
  style: Option<StyleML>,
  title: &str,
  rect: RectML,
) {
  with_frame(|f| {
    let style = match style {
      Some(style) => Style::from(style),
      None => Style::default(),
    };
    let span = Span::styled(title, style);
    let block = Block::default()
      .border_style(style)
      .title(span)
      .borders(Borders::ALL);

    f.render_widget(block, rect.tui());
  })
}

#[ocaml::func]
pub fn tui_render_string(
  _f: ocaml::Value,
  style: Option<StyleML>,
  s: &str,
  rect: RectML,
) {
  with_frame(|f| {
    let style = match style {
      Some(style) => Style::from(style),
      None => Style::default(),
    };

    let w = RenderWidget::new(|buf| {
      buf.set_stringn(rect.x, rect.y, s, rect.w.into(), style);
    });

    if f.size().width == 0 || f.size().height == 0 {
    } else {
      f.render_widget(w, rect.tui());
    }
  })
}
