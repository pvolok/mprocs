use termwiz::escape::csi::CursorStyle;
use tui::{
  layout::{Margin, Rect},
  style::{Color, Style},
  text::{Line, Span, Text},
  widgets::{Clear, Paragraph, Widget, Wrap},
  Frame,
};

use crate::{
  proc::{view::ProcViewFrame, CopyMode, Pos, ReplySender},
  state::{Scope, State},
  theme::Theme,
};

pub fn render_term(
  area: Rect,
  frame: &mut Frame,
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
    match proc.copy_mode() {
      CopyMode::None(_) => (),
      CopyMode::Active(_, _, _) => {
        title.push(Span::raw(" "));
        title.push(Span::styled("COPY MODE", theme.copy_mode_label()));
      }
    };

    let block = theme.pane(active).title(Line::from(title));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    match &proc.lock_view() {
      ProcViewFrame::Empty => (),
      ProcViewFrame::Vt(vt) => {
        let (screen, cursor) = match proc.copy_mode() {
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
          CopyMode::Active(screen, start, end) => {
            let pos = end.as_ref().unwrap_or(start);
            let y = area.y as i32 + 1 + (pos.y + screen.scrollback() as i32);
            let cursor = if y >= 0 {
              Some((area.x + 1 + pos.x as u16, y as u16))
            } else {
              None
            };
            (screen, cursor)
          }
        };

        let term = UiTerm::new(screen, proc.copy_mode());
        frame.render_widget(
          term,
          area.inner(Margin {
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
      ProcViewFrame::Err(err) => {
        let text =
          Text::styled(*err, Style::default().fg(tui::style::Color::Red));
        frame.render_widget(
          Paragraph::new(text).wrap(Wrap { trim: false }),
          area.inner(Margin {
            vertical: 1,
            horizontal: 1,
          }),
        );
      }
    }
  }
}

pub struct UiTerm<'a> {
  screen: &'a crate::vt100::Screen<ReplySender>,
  copy_mode: &'a CopyMode,
}

impl<'a> UiTerm<'a> {
  pub fn new(
    screen: &'a crate::vt100::Screen<ReplySender>,
    copy_mode: &'a CopyMode,
  ) -> Self {
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
          *to_cell = cell.to_tui();
          if !cell.has_contents() {
            to_cell.set_char(' ');
          }

          let copy_mode = match self.copy_mode {
            CopyMode::None(_) => None,
            CopyMode::Active(_, start, end) => {
              Some((start, end.as_ref().unwrap_or(start)))
            }
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
              to_cell.fg = Color::Black; // Black
              to_cell.bg = Color::Cyan; // Cyan
            }
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
