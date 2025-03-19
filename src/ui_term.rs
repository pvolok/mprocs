use termwiz::escape::csi::CursorStyle;
use tui::{
  layout::{Margin, Rect},
  style::{Color, Style, Modifier},
  text::{Line, Span, Text},
  widgets::{Clear, Paragraph, Widget, Wrap},
  Frame,
};

use crate::{
  proc::{handle::ProcViewFrame, CopyMode, Pos},
  state::{Scope, State},
  theme::Theme,
  clipboard::set_clipboard_content,
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
      CopyMode::Start(_, _) | CopyMode::Range(_, _, _) => {
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
          CopyMode::Start(screen, pos) | CopyMode::Range(screen, _, pos) => {
            let y = area.y as i32 + 1 + (pos.y + screen.scrollback() as i32);
            let cursor = if y >= 0 {
              Some((area.x + 1 + pos.x as u16, y as u16))
            } else {
              None
            };
            (screen, cursor)
          }
        };

        let term = UiTerm::with_state(&screen, proc.copy_mode(), state);
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
      ProcViewFrame::Err(err) => {
        let text =
          Text::styled(*err, Style::default().fg(tui::style::Color::Red));
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
  state: Option<&'a State>,
}

impl<'a> UiTerm<'a> {
  pub fn new(screen: &'a vt100::Screen, copy_mode: &'a CopyMode) -> Self {
    Self { 
      screen, 
      copy_mode,
      state: None,
    }
  }

  pub fn with_state(screen: &'a vt100::Screen, copy_mode: &'a CopyMode, state: &'a State) -> Self {
    Self {
      screen,
      copy_mode,
      state: Some(state),
    }
  }

  fn copy_all_content(&self) -> String {
    let mut content = String::new();
    let size = self.screen.size();
    
    for row in 0..size.0 {
      for col in 0..size.1 {
        if let Some(cell) = self.screen.cell(row, col) {
          content.push_str(&cell.contents());
        }
      }
      content.push('\n');
    }
    content
  }

  fn find_matches(&self, query: &str) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let size = self.screen.size();
    
    for row in 0..size.0 {
      let mut line = String::new();
      for col in 0..size.1 {
        if let Some(cell) = self.screen.cell(row, col) {
          line.push_str(&cell.contents());
        }
      }
      
      // Find all matches in this line
      let mut start = 0;
      while let Some(pos) = line[start..].find(query) {
        let abs_pos = start + pos;
        matches.push((row as usize, abs_pos));
        start = abs_pos + 1;
      }
    }
    matches
  }

  fn highlight_matches(&self, buf: &mut tui::buffer::Buffer, matches: &[(usize, usize)], current_match: Option<usize>) {
    let highlight_style = Style::default()
      .bg(Color::Yellow)
      .fg(Color::Black)
      .add_modifier(Modifier::BOLD);
    
    let current_highlight_style = Style::default()
      .bg(Color::Green)
      .fg(Color::Black)
      .add_modifier(Modifier::BOLD);

    for (idx, &(row, col)) in matches.iter().enumerate() {
      let style = if Some(idx) == current_match {
        current_highlight_style
      } else {
        highlight_style
      };

      // Apply highlight style to the matched text
      let cell = buf.get_mut(col as u16, row as u16);
      let mut new_cell = cell.clone();
      new_cell.set_style(style);
      *cell = new_cell;
    }
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

    // After rendering the terminal content, highlight any search matches
    if let Some(state) = self.state {
      if state.search_active && !state.search_query.is_empty() {
        let matches = self.find_matches(&state.search_query);
        self.highlight_matches(buf, &matches, state.current_match);
      }
    }
  }
}

pub fn term_check_hit(area: Rect, x: u16, y: u16) -> bool {
  area.x <= x
    && area.x + area.width >= x + 1
    && area.y <= y
    && area.y + area.height >= y + 1
}

pub fn copy_all_terminal_content(state: &State) -> anyhow::Result<()> {
  if let Some(proc) = state.get_current_proc() {
    if let ProcViewFrame::Vt(vt) = &proc.lock_view() {
      let screen = vt.screen();
      let ui_term = UiTerm::new(&screen, proc.copy_mode());
      let content = ui_term.copy_all_content();
      set_clipboard_content(&content)?;
    }
  }
  Ok(())
}
