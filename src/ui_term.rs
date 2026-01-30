use crate::{
  proc::{view::ProcViewFrame, CopyMode, Pos},
  state::{Scope, State},
  vt100::{attrs::Attrs, grid::Rect, Color, Grid, Screen},
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

        render_screen(screen, proc.copy_mode(), area.inner(1), grid);

        if active {
          if let Some(cursor) = cursor {
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
  }
}

fn render_screen(
  screen: &Screen,
  copy_mode: &CopyMode,
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
