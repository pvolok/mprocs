use std::{
  io,
  sync::{Arc, RwLock},
};

use tui::{
  backend::CrosstermBackend,
  layout::{Margin, Rect},
  style::{Color, Modifier, Style},
  text::{Span, Text},
  widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
  Frame,
};

use crate::{
  proc::ProcState,
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

    match &proc.inst {
      ProcState::None => (),
      ProcState::Some(inst) => {
        let term = UiTerm::new(inst.vt.clone());
        frame.render_widget(
          term,
          area.inner(&Margin {
            vertical: 1,
            horizontal: 1,
          }),
        );

        if active {
          let vt = inst.vt.read().unwrap();
          let screen = vt.screen();
          let cursor = screen.cursor_position();
          if !screen.hide_cursor() {
            frame.set_cursor(area.x + 1 + cursor.1, area.y + 1 + cursor.0);
          }
        }
      }
      ProcState::Error(err) => {
        let text = Text::styled(err, Style::default().fg(Color::Red));
        frame.render_widget(
          Paragraph::new(text).wrap(Wrap { trim: false }),
          area.inner(&Margin {
            vertical: 1,
            horizontal: 1,
          }),
        );
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

    let scrollback = screen.scrollback();
    if scrollback > 0 {
      let str = format!(" -{} ", scrollback);
      let width = str.len() as u16;
      let span = Span::styled(
        str,
        Style::reset().bg(Color::LightYellow).fg(Color::Black),
      );
      let x = area.x + area.width - width;
      let y = area.y;
      buf.set_span(x, y, &span, width);
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
