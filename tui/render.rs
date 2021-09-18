use std::io;

use ocaml::{Error, Pointer};
use tui::{
    backend::CrosstermBackend,
    style::Style,
    text::Span,
    widgets::{Block, Borders},
};

use crate::{layout::MLRect, render_widget::RenderWidget, style::StyleML, terminal::Term};

type Backend = CrosstermBackend<io::Stdout>;

#[ocaml::func]
pub fn tui_render(mut ptr: Pointer<Term>, draw: ocaml::Value) -> Result<(), Error> {
    let term = ptr.as_mut();

    let mut result = Ok(());

    term.terminal.draw(|f| {
        let f_val = unsafe { ocaml::Value::alloc_abstract_ptr(f) };
        result = unsafe { draw.call(gc, f_val) }.map(|_| ());
        drop(f);
    })?;

    result
}

#[ocaml::func]
pub fn tui_render_frame_size(f: ocaml::Value) -> MLRect {
    let f = unsafe { (f.abstract_ptr_val_mut() as *mut tui::Frame<Backend>).as_mut() }.unwrap();

    MLRect::of_tui(f.size())
}

#[ocaml::func]
pub fn tui_render_block(f: ocaml::Value, style: Option<StyleML>, title: &str, rect: MLRect) {
    let f = unsafe { (f.abstract_ptr_val_mut() as *mut tui::Frame<Backend>).as_mut() }.unwrap();
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
}

#[ocaml::func]
pub fn tui_render_string(f: ocaml::Value, style: Option<StyleML>, s: &str, rect: MLRect) {
    let f = unsafe { (f.abstract_ptr_val_mut() as *mut tui::Frame<Backend>).as_mut() }.unwrap();

    let style = match style {
        Some(style) => Style::from(style),
        None => Style::default(),
    };

    let w = RenderWidget::new(|buf| {
        buf.set_stringn(rect.x, rect.y, s, rect.w.into(), style);
    });

    f.render_widget(w, rect.tui());
}
