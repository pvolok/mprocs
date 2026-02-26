use std::collections::HashSet;

use crate::{
  proc::{
    view::{ProcViewFrame, SearchState},
    CopyMode, Pos,
  },
  state::{Scope, State},
  vt100::{attrs::Attrs, grid::Rect, Color, Grid, Screen},
  widgets::text_input::render_text_input,
};

pub fn render_term(area: Rect, grid: &mut Grid, state: &mut State) {
  if area.width < 3 || area.height < 3 {
    return;
  }

  let active = match state.scope {
    Scope::Procs => false,
    Scope::Term | Scope::TermZoom => true,
  };

  if let Some(proc) = state.get_current_proc() {
    let border_type = match active {
      true => crate::vt100::grid::BorderType::Thick,
      false => crate::vt100::grid::BorderType::Plain,
    };
    grid.draw_block(area, border_type, crate::vt100::attrs::Attrs::default());

    let mut top_line = Rect {
      x: area.x + 1,
      y: area.y,
      width: area.width - 2,
      height: 1,
    };
    let r =
      grid.draw_text(top_line, "Terminal", Attrs::default().set_bold(active));
    top_line = top_line.move_left(r.width as i32);
    match proc.copy_mode() {
      CopyMode::None(_) => (),
      CopyMode::Active(_, _, _) => {
        let r = grid.draw_text(top_line, " ", Attrs::default());
        top_line = top_line.move_left(r.width as i32);
        let _r = grid.draw_text(
          top_line,
          "COPY MODE",
          Attrs::default()
            .fg(Color::BLACK)
            .bg(Color::YELLOW)
            .set_bold(true),
        );
        // top_line = top_line.move_left(r.width as i32);
      }
    };

    // Determine if search is active and compute areas
    let search_active = proc.search.is_some();
    let inner = area.inner(1);
    let (screen_area, search_bar_area) = if search_active && inner.height > 1 {
      let screen = Rect {
        height: inner.height - 1,
        ..inner
      };
      let bar = Rect {
        y: inner.y + inner.height - 1,
        height: 1,
        ..inner
      };
      (screen, Some(bar))
    } else {
      (inner, None)
    };

    // Build search highlight set
    let search_highlights = build_search_highlights(proc.search.as_ref(), proc);

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

        render_screen(
          screen,
          proc.copy_mode(),
          &search_highlights,
          screen_area,
          grid,
        );

        if active {
          if search_active {
            // Place cursor in search bar instead of terminal
          } else if let Some(cursor) = cursor {
            grid.cursor_pos = Some(crate::vt100::grid::Pos {
              col: cursor.0,
              row: cursor.1,
            });
            grid.cursor_style = vt.screen().cursor_style();
          }
        }
      }
      ProcViewFrame::Err(err) => {
        grid.draw_text(area.inner(1), *err, Attrs::default().fg(Color::RED));
      }
    }

    // Render search bar
    if let Some(bar_area) = search_bar_area {
      if let Some(proc) = state.get_current_proc_mut() {
        if let Some(search) = &mut proc.search {
          render_search_bar(search, bar_area, grid);
          if active {
            grid.cursor_style = crate::protocol::CursorStyle::SteadyBar;
          }
        }
      }
    }
  }
}

fn build_search_highlights(
  search: Option<&SearchState>,
  proc: &crate::proc::view::ProcView,
) -> HashSet<(u16, u16)> {
  let mut highlights = HashSet::new();
  let search = match search {
    Some(s) if !s.matches.is_empty() => s,
    _ => return highlights,
  };

  let vt_ref = match &proc.vt {
    Some(vt) => vt,
    None => return highlights,
  };
  let vt = match vt_ref.read() {
    Ok(vt) => vt,
    Err(_) => return highlights,
  };
  let screen = vt.screen();
  let abs_start = screen.visible_row_abs_start();
  let height = screen.size().height as usize;
  let query_len = search.query_len();

  for &(abs_row, col_offset) in &search.matches {
    if abs_row >= abs_start && abs_row < abs_start + height {
      let visible_row = (abs_row - abs_start) as u16;
      for c in 0..query_len {
        highlights.insert((visible_row, (col_offset + c) as u16));
      }
    }
  }

  highlights
}

fn render_search_bar(
  search: &mut SearchState,
  area: Rect,
  grid: &mut Grid,
) {
  // Draw "/ " prefix
  let prefix = "/ ";
  let prefix_len = prefix.len() as u16;
  grid.fill_area(area, ' ', Attrs::default());
  grid.draw_text(area, prefix, Attrs::default().set_bold(true));

  let input_area = Rect {
    x: area.x + prefix_len,
    width: area.width.saturating_sub(prefix_len),
    ..area
  };

  // Draw match count on the right side
  let match_info = if search.matches.is_empty() {
    if search.input.value().is_empty() {
      String::new()
    } else {
      " 0/0 ".to_string()
    }
  } else {
    format!(" {}/{} ", search.current + 1, search.matches.len())
  };

  let info_len = match_info.len() as u16;
  let text_input_area = if !match_info.is_empty() && input_area.width > info_len
  {
    let info_area = Rect {
      x: input_area.x + input_area.width - info_len,
      width: info_len,
      ..input_area
    };
    grid.draw_text(
      info_area,
      &match_info,
      Attrs::default().fg(Color::BLACK).bg(Color::BRIGHT_YELLOW),
    );
    Rect {
      width: input_area.width - info_len,
      ..input_area
    }
  } else {
    input_area
  };

  let mut cursor_pos = (0u16, 0u16);
  render_text_input(&mut search.input, text_input_area, grid, &mut cursor_pos);
  grid.cursor_pos = Some(crate::vt100::grid::Pos {
    col: cursor_pos.0,
    row: cursor_pos.1,
  });
}

fn render_screen(
  screen: &Screen,
  copy_mode: &CopyMode,
  search_highlights: &HashSet<(u16, u16)>,
  area: Rect,
  grid: &mut Grid,
) {
  for row in 0..area.height {
    for col in 0..area.width {
      let to_cell = if let Some(cell) =
        grid.drawing_cell_mut(crate::vt100::grid::Pos {
          col: area.x + col,
          row: area.y + row,
        }) {
        cell
      } else {
        continue;
      };
      if let Some(cell) = screen.cell(row, col) {
        *to_cell = cell.clone();
        if !cell.has_contents() {
          to_cell.set_str(" ");
        }

        let copy_mode = match copy_mode {
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
            to_cell.set_attrs(
              Attrs::default()
                .fg(crate::vt100::Color::BLACK)
                .bg(crate::vt100::Color::CYAN),
            );
          }
        }

        // Search highlighting
        if search_highlights.contains(&(row, col)) {
          to_cell.set_attrs(
            Attrs::default()
              .fg(Color::BLACK)
              .bg(Color::YELLOW),
          );
        }
      } else {
        // Out of bounds.
        to_cell.set_str("?");
      }
    }
  }

  let scrollback = screen.scrollback();
  if scrollback > 0 {
    let str = format!(" -{} ", scrollback);
    let width = str.len() as u16;
    let x = area.x + area.width - width;
    let y = area.y;
    grid.draw_text(
      Rect::new(x, y, width, 1),
      str.as_str(),
      Attrs::default().fg(Color::BLACK).bg(Color::BRIGHT_YELLOW),
    );
  }
}

pub fn term_check_hit(area: Rect, x: u16, y: u16) -> bool {
  area.x <= x
    && area.x + area.width >= x + 1
    && area.y <= y
    && area.y + area.height >= y + 1
}
