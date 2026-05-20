use crate::color;
use crate::console::state::ConsoleState;
use crate::console::views::frame::draw_frame;
use crate::console::views::pane::Pane;
use crate::kernel::task::TaskStatus;
use crate::term::Grid;
use crate::term::attrs::Attrs;
use crate::term::grid::Rect;
use crate::term::scroll_offset;

pub struct ProcsPane;

impl Pane for ProcsPane {
  fn render(
    &mut self,
    grid: &mut Grid,
    area: Rect,
    state: &mut ConsoleState,
    focused: bool,
  ) {
    draw_frame(grid, area, " Processes ", focused);
    if area.width < 2 || area.height < 2 {
      return;
    }

    let inner = area.inner(1);
    if state.tasks.is_empty() {
      grid.draw_text(
        area.inner((1, 2)),
        "No tasks",
        Attrs::default().fg(color!("#aaaaaa")),
      );
      return;
    }

    let max_rows = inner.height as usize;
    let start = scroll_offset(state.selected, state.tasks.len(), max_rows);

    for (i, task) in state.tasks.iter().enumerate().skip(start).take(max_rows) {
      let Some(row) = inner.row((i - start) as u16) else {
        break;
      };
      let is_selected = i == state.selected;
      let bg = if is_selected {
        color!("#303030")
      } else {
        color!("#111111")
      };

      let (status_col, path_col) = row.split_v(2);

      let (status_char, status_color) = match task.status {
        TaskStatus::Running => ("●", color!("#4ec55e")),
        TaskStatus::NotStarted => ("○", color!("#888888")),
        TaskStatus::Exited(0) => ("○", color!("#5a8ee0")),
        TaskStatus::Exited(_) => ("○", color!("#e05a5a")),
      };
      grid.draw_line(
        status_col,
        status_char,
        Attrs::default().fg(status_color).bg(bg),
      );
      grid.draw_line(
        path_col,
        &task.path,
        Attrs::default().fg(color!("#e0e0e0")).bg(bg),
      );
    }
  }
}
