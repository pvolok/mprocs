use crate::{
  event::AppEvent,
  keymap::{Keymap, KeymapGroup},
  vt100::{attrs::Attrs, grid::Rect, Color, Grid},
};

pub fn render_zoom_tip(area: Rect, grid: &mut Grid, keymap: &Keymap) {
  if area.height == 0 {
    return;
  }

  let events = vec![
    AppEvent::FocusTerm,
    AppEvent::ToggleFocus,
    AppEvent::FocusProcs,
  ];
  let key = events
    .into_iter()
    .find_map(|event| keymap.resolve_key(KeymapGroup::Term, &event));

  let attrs = Attrs::default().fg(Color::BLACK).bg(Color::YELLOW);
  let r = if let Some(key) = key {
    grid.draw_text(
      area,
      format!(" To exit zoom mode press {}", key.to_string()).as_str(),
      attrs,
    )
  } else {
    grid.draw_text(area, " No key bound to exit the zoom mode", attrs)
  };

  grid.fill_area(area.inner((0, 0, 0, r.width)), ' ', attrs);
}
