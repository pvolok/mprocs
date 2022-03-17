use std::{
  io,
  sync::{Arc, RwLock},
};

use tui::{
  backend::CrosstermBackend,
  layout::{Margin, Rect},
  style::{Color, Modifier, Style},
  text::Span,
  widgets::{Block, BorderType, Borders, Widget},
  Frame,
};

use crate::{
  state::{Scope, State},
  theme::Theme,
};

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_term(area: Rect, frame: &mut Frame<Backend>, state: &mut State) {
  let theme = Theme::default();

  let active = state.scope == Scope::Term;

  if let Some(proc) = state.get_current_proc() {
    let block = Block::default()
      .title(Span::styled("Terminal", theme.pane_title(active)))
      .borders(Borders::ALL)
      .border_style(theme.pane_border(active))
      .border_type(BorderType::Rounded)
      .style(Style::default().bg(Color::Black));
    frame.render_widget(block, area);

    let term = UiTerm::new(proc.inst.vt.clone());
    frame.render_widget(
      term,
      area.inner(&Margin {
        vertical: 1,
        horizontal: 1,
      }),
    );

    {
      let vt = proc.inst.vt.read().unwrap();
      let screen = vt.screen();
      let cursor = screen.cursor_position();
      if !screen.hide_cursor() {
        frame.set_cursor(area.x + 1 + cursor.1, area.y + 1 + cursor.0);
      }
    }
  }
}

pub struct UiTerm {
  vt: Arc<RwLock<vt100::Parser>>,
}

impl UiTerm {
  pub fn new(vt: Arc<RwLock<vt100::Parser>>) -> Self {
    UiTerm { vt }
  }
}

impl Widget for UiTerm {
  fn render(self, area: Rect, buf: &mut tui::buffer::Buffer) {
    let vt = self.vt.read().unwrap();
    let screen = vt.screen();

    for row in 0..area.height {
      for col in 0..area.width {
        let to_cell = buf.get_mut(area.x + col, area.y + row);
        if let Some(cell) = screen.cell(row, col) {
          if cell.has_contents() {
            let mut mods = Modifier::empty();
            mods.set(Modifier::BOLD, cell.bold());
            mods.set(Modifier::ITALIC, cell.italic());
            mods.set(Modifier::REVERSED, cell.inverse());
            mods.set(Modifier::UNDERLINED, cell.underline());

            let style = Style {
              fg: conv_color(cell.fgcolor()),
              bg: conv_color(cell.bgcolor()),
              add_modifier: mods,
              sub_modifier: Modifier::empty(),
            };
            to_cell.set_style(style);

            to_cell.set_symbol(&cell.contents());
          } else {
            // Cell doesn't have content.
            to_cell.set_char(' ');
          }
        } else {
          // Out of bounds.
          to_cell.set_char('?');
        }
      }
    }
  }
}

fn conv_color(color: vt100::Color) -> Option<tui::style::Color> {
  match color {
    vt100::Color::Default => None,
    vt100::Color::Idx(index) => Some(tui::style::Color::Indexed(index)),
    vt100::Color::Rgb(r, g, b) => Some(tui::style::Color::Rgb(r, g, b)),
  }
}
