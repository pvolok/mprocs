use termwiz::escape::csi::CursorStyle;
use tui::{
  layout::{Margin, Rect},
  style::Style,
  text::{Span, Spans, Text},
  widgets::{Clear, Paragraph, Widget, Wrap},
  Frame,
};

use crate::{
  dk_screen::{attrs::Color, cell::Cell},
  proc::{CopyMode, Pos, ProcState},
  protocol::ProxyBackend,
  state::{Scope, State},
  theme::Theme,
};

type Backend = ProxyBackend;

pub fn render_term(
  area: Rect,
  frame: &mut Frame<Backend>,
  state: &mut State,
  cursor_style: &mut CursorStyle,
) {
  if area.width < 3 || area.height < 3 {
    return;
  }

  let theme = Theme::default();

  let active = match state.scope {
    Scope::Procs => false,
    Scope::Term | Scope::TermZoom => true,
  };

  if let Some(proc) = state.get_current_proc() {
    let mut title = Vec::with_capacity(4);
    title.push(Span::styled("Terminal", theme.pane_title(active)));
    match proc.copy_mode {
      CopyMode::None(_) => (),
      CopyMode::Start(_, _) | CopyMode::Range(_, _, _) => {
        title.push(Span::raw(" "));
        title.push(Span::styled("COPY MODE", theme.copy_mode_label()));
      }
    };

    let block = theme.pane(active).title(Spans::from(title));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    match &proc.inst {
      ProcState::None => (),
      ProcState::Some(inst) => {
        let vt = inst.vt.read();
        match vt {
          Ok(vt) => {
            let (screen, cursor) = match &proc.copy_mode {
              CopyMode::None(_) => {
                let screen = vt.screen();
                let cursor = if screen.hide_cursor() {
                  None
                } else {
                  let cursor = screen.cursor_position();
                  Some((area.x + 1 + cursor.1, area.y + 1 + cursor.0))
                };
                (screen, cursor)
              }
              CopyMode::Start(screen, pos)
              | CopyMode::Range(screen, _, pos) => {
                let y =
                  area.y as i32 + 1 + (pos.y + screen.scrollback() as i32);
                let cursor = if y >= 0 {
                  Some((area.x + 1 + pos.x as u16, y as u16))
                } else {
                  None
                };
                (screen, cursor)
              }
            };

            let term = UiTerm::new(screen, &proc.copy_mode);
            frame.render_widget(
              term,
              area.inner(&Margin {
                vertical: 1,
                horizontal: 1,
              }),
            );

            if active {
              if let Some(cursor) = cursor {
                frame.set_cursor(cursor.0, cursor.1);
                *cursor_style = vt.screen().cursor_style();
              }
            }
          }
          Err(err) => {
            let text = Text::styled(
              err.to_string(),
              Style::default().fg(tui::style::Color::Red),
            );
            frame.render_widget(
              Paragraph::new(text),
              area.inner(&Margin {
                vertical: 1,
                horizontal: 1,
              }),
            );
          }
        }
      }
      ProcState::Error(err) => {
        let text =
          Text::styled(err, Style::default().fg(tui::style::Color::Red));
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

pub struct UiTerm<'a> {
  screen: &'a vt100::Screen,
  copy_mode: &'a CopyMode,
}

impl<'a> UiTerm<'a> {
  pub fn new(screen: &'a vt100::Screen, copy_mode: &'a CopyMode) -> Self {
    UiTerm { screen, copy_mode }
  }
}

impl Widget for UiTerm<'_> {
  fn render(self, area: Rect, buf: &mut tui::buffer::Buffer) {
    let screen = self.screen;

    for row in 0..area.height {
      for col in 0..area.width {
        let to_cell = buf.get_mut(area.x + col, area.y + row);
        if let Some(cell) = screen.cell(row, col) {
          if cell.has_contents() {
            let mut new_cell = Cell::from_vt100(cell);

            let copy_mode = match self.copy_mode {
              CopyMode::None(_) => None,
              CopyMode::Start(_, start) => Some((start, start)),
              CopyMode::Range(_, start, end) => Some((start, end)),
            };
            if let Some((start, end)) = copy_mode {
              if Pos::within(
                start,
                end,
                &Pos {
                  y: (row as i32) - screen.scrollback() as i32,
                  x: col as i32,
                },
              ) {
                new_cell.attrs.fg = Color::Idx(0); // Black
                new_cell.attrs.bg = Color::Idx(6); // Cyan
              }
            }

            *to_cell = new_cell.to_tui();
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
        Style::reset()
          .bg(tui::style::Color::LightYellow)
          .fg(tui::style::Color::Black),
      );
      let x = area.x + area.width - width;
      let y = area.y;
      buf.set_span(x, y, &span, width);
    }
  }
}

pub fn term_check_hit(area: Rect, x: u16, y: u16) -> bool {
  area.x <= x
    && area.x + area.width >= x + 1
    && area.y <= y
    && area.y + area.height >= y + 1
}
