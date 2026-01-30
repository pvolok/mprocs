use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

use crate::{
  config::Config,
  state::{Scope, State},
  vt100::{
    attrs::Attrs,
    grid::{BorderType, Rect},
    Color, Grid,
  },
};

pub fn render_procs(
  area: Rect,
  grid: &mut Grid,
  state: &mut State,
  config: &Config,
) {
  state.procs_list.fit(area.inner(1), state.procs.len());

  if area.width <= 2 {
    return;
  }

  let active = state.scope == Scope::Procs;

  grid.draw_block(
    area.into(),
    if active {
      BorderType::Thick
    } else {
      BorderType::Plain
    },
    Attrs::default(),
  );
  let title_area = Rect {
    x: area.x + 1,
    y: area.y,
    width: area.width - 2,
    height: 1,
  };
  let r = grid.draw_text(
    title_area,
    config.proc_list_title.as_str(),
    if active {
      Attrs::default().set_bold(true)
    } else {
      Attrs::default()
    },
  );
  if state.quitting {
    let area = title_area.inner((0, 0, 0, r.width + 1));
    grid.draw_text(
      area,
      "QUITTING",
      Attrs::default()
        .fg(Color::BLACK)
        .bg(Color::RED)
        .set_bold(true),
    );
  }

  let range = state.procs_list.visible_range();
  for (row, index) in range.enumerate() {
    let proc = if let Some(proc) = state.procs.get(index) {
      proc
    } else {
      continue;
    };

    let selected = index == state.selected();
    let attrs = if selected {
      Attrs::default().bg(crate::vt100::Color::Idx(240))
    } else {
      Attrs::default()
    };
    let mut row_area = crate::vt100::grid::Rect {
      x: area.x + 1,
      y: area.y + 1 + row as u16,
      width: area.width.saturating_sub(2),
      height: 1,
    };

    let r = grid.draw_text(row_area, if selected { "•" } else { " " }, attrs);
    row_area.x += r.width;
    row_area.width = row_area.width.saturating_sub(r.width);

    let r = grid.draw_text(row_area, proc.name(), attrs);
    row_area.x += r.width;
    row_area.width = row_area.width.saturating_sub(r.width);

    let (status_text, status_attrs) = if proc.is_up() {
      (
        Cow::from(" UP "),
        attrs.clone().set_bold(true).fg(Color::BRIGHT_GREEN),
      )
    } else {
      match proc.exit_code() {
        Some(0) => {
          (Cow::from(" DOWN (0)"), attrs.clone().fg(Color::BRIGHT_BLUE))
        }
        Some(exit_code) => (
          Cow::from(format!(" DOWN ({})", exit_code)),
          attrs.clone().fg(Color::BRIGHT_RED),
        ),
        None => (Cow::from(" DOWN "), attrs.clone().fg(Color::BRIGHT_RED)),
      }
    };
    let status_width = status_text.width() as u16;
    let r = grid.draw_text(
      Rect {
        x: row_area.x.max(row_area.x + row_area.width - status_width),
        width: status_width.min(row_area.width),
        ..row_area
      },
      &status_text,
      status_attrs,
    );
    row_area.width = row_area.width.saturating_sub(r.width);

    grid.fill_area(row_area, ' ', attrs);
  }
}

pub fn procs_get_clicked_index(
  area: Rect,
  x: u16,
  y: u16,
  state: &State,
) -> Option<usize> {
  let inner = area.inner(1);
  if procs_check_hit(area, x, y) {
    let index = y - inner.y;
    let scroll = (state.selected() + 1).saturating_sub(inner.height as usize);
    let index = index as usize + scroll;
    if index < state.procs.len() {
      return Some(index);
    }
  }
  None
}

pub fn procs_check_hit(area: Rect, x: u16, y: u16) -> bool {
  area.x < x
    && area.x + area.width > x + 1
    && area.y < y
    && area.y + area.height > y + 1
}
