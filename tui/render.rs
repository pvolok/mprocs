use std::{ffi::CStr, io, os::raw::c_char};

use tui::{
  backend::CrosstermBackend,
  style::Style,
  text::Span,
  widgets::{Block, Borders},
  Frame,
};

use crate::conv::style_of_c;
use crate::ctypes::types as cty;
use crate::render_widget::RenderWidget;
use crate::terminal::use_term;

type Backend = CrosstermBackend<io::Stdout>;

fn with_frame<F, R>(f: F) -> R
where
  F: FnOnce(&mut Frame<Backend>) -> R,
{
  use_term(|term| {
    let mut frame = term.terminal.get_frame();
    f(&mut frame)
  })
}

#[no_mangle]
pub extern "C" fn tui_render_start() {
  crate::terminal::use_term(|term| term.terminal.autoresize()).unwrap()
}

#[no_mangle]
pub extern "C" fn tui_render_end() {
  crate::terminal::use_term(|term| -> Result<(), io::Error> {
    term.terminal.draw(|_f| ())?;
    Ok(())
  })
  .unwrap();
}

#[no_mangle]
pub extern "C" fn tui_frame_size() -> cty::Rect {
  with_frame(|f| cty::Rect::from(f.size()))
}

#[no_mangle]
pub fn tui_render_block(
  style_opt: *const cty::Style,
  title: *const c_char,
  rect: cty::Rect,
) {
  let style = if style_opt.is_null() {
    None
  } else {
    Some(style_of_c(unsafe { *style_opt }))
  };
  let title = unsafe { CStr::from_ptr(title) }
    .to_str()
    .unwrap_or("<bad_utf8>");

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

    f.render_widget(block, rect.into());
  })
}

#[no_mangle]
pub fn tui_render_string(
  style_opt: *const cty::Style,
  s: *const c_char,
  rect: cty::Rect,
) {
  let style = if style_opt.is_null() {
    None
  } else {
    Some(style_of_c(unsafe { *style_opt }))
  };
  let s = unsafe { CStr::from_ptr(s) }
    .to_str()
    .unwrap_or("<bad_utf8>");

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
      f.render_widget(w, rect.into());
    }
  })
}
