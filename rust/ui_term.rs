use std::{
  borrow::BorrowMut,
  io,
  sync::{Arc, RwLock},
};

use crossterm::cursor::MoveToRow;
use tui::{
  backend::CrosstermBackend,
  layout::{Margin, Rect},
  style::{Color, Modifier, Style},
  text::{Span, Spans},
  widgets::{Block, BorderType, Borders, List, ListItem, ListState, Widget},
  Frame,
};

use crate::{proc::Proc, state::State, theme::Theme};

type Backend = CrosstermBackend<io::Stdout>;

pub fn render_term(area: Rect, frame: &mut Frame<Backend>, state: &mut State) {
  let theme = Theme::default();

  if let Some(proc) = state.get_current_proc() {
    let block = Block::default()
      .title("Terminal")
      .borders(Borders::ALL)
      .border_style(Style::default().fg(Color::White))
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

    let width = area.width;
    let height = area.height;
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

fn create_proc_item<'a>(proc: &Proc, is_cur: bool, width: u16) -> ListItem<'a> {
  let status = Span::styled(
    " UP",
    Style::default()
      .fg(Color::LightGreen)
      .add_modifier(Modifier::BOLD),
  );

  let mark = if is_cur {
    Span::raw(">>")
  } else {
    Span::raw("  ")
  };

  let mut name = proc.name.clone();
  let name_max = width as usize - mark.width() - status.width();
  let name_len = name.chars().count();
  if name_len > name_max {
    name.truncate(
      name
        .char_indices()
        .nth(name_max)
        .map_or(name.len(), |(n, _)| n),
    )
  }
  if name_len < name_max {
    for _ in name_len..name_max {
      name.push(' ');
    }
  }
  let name = Span::raw(name);

  ListItem::new(Spans::from(vec![mark, name, status]))
}
