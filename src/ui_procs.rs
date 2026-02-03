use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

use crate::{
  config::Config,
  state::{Scope, SidebarItem, State},
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
  // Fit the list to sidebar items count instead of procs count
  state.procs_list.fit(area.inner(1), state.sidebar_items.len());

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
  for (row, sidebar_index) in range.enumerate() {
    let sidebar_item = if let Some(item) = state.sidebar_items.get(sidebar_index)
    {
      item.clone()
    } else {
      continue;
    };

    let selected = sidebar_index == state.selected();
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

    match sidebar_item {
      SidebarItem::Group { group_index } => {
        // Render group header
        if let Some(group) = state.groups.get(group_index) {
          let indicator = if group.collapsed { "▸" } else { "▾" };
          let r = grid.draw_text(
            row_area,
            if selected { indicator } else { indicator },
            attrs.clone().set_bold(true),
          );
          row_area.x += r.width;
          row_area.width = row_area.width.saturating_sub(r.width);

          let r = grid.draw_text(row_area, " ", attrs);
          row_area.x += r.width;
          row_area.width = row_area.width.saturating_sub(r.width);

          let r =
            grid.draw_text(row_area, &group.name, attrs.clone().set_bold(true));
          row_area.x += r.width;
          row_area.width = row_area.width.saturating_sub(r.width);

          grid.fill_area(row_area, ' ', attrs);
        }
      }
      SidebarItem::Process { proc_index } => {
        // Check if this process is in a group (for indentation)
        let is_grouped = state
          .groups
          .iter()
          .any(|g| g.proc_indices.contains(&proc_index));

        let proc = if let Some(proc) = state.procs.get(proc_index) {
          proc
        } else {
          continue;
        };

        // Add indentation for grouped processes
        if is_grouped {
          let r = grid.draw_text(row_area, "  ", attrs);
          row_area.x += r.width;
          row_area.width = row_area.width.saturating_sub(r.width);
        }

        let r =
          grid.draw_text(row_area, if selected { "*" } else { " " }, attrs);
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
  }
}

/// Returns the sidebar index of the clicked item
pub fn procs_get_clicked_index(
  area: Rect,
  x: u16,
  y: u16,
  state: &State,
) -> Option<usize> {
  let inner = area.inner(1);
  if procs_check_hit(area, x, y) {
    let row_offset = y - inner.y;
    let top_index = state.procs_list.top_index();
    let index = row_offset as usize + top_index;
    if index < state.sidebar_items.len() {
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
