use crate::{
  encode_term::print_key,
  event::AppEvent,
  keymap::{Keymap, KeymapGroup},
  state::State,
  vt100::{attrs::Attrs, grid::Rect, Color, Grid},
};

pub fn render_keymap(
  area: Rect,
  grid: &mut Grid,
  state: &mut State,
  keymap: &Keymap,
) {
  if area.width <= 3 || area.height < 3 {
    return;
  }

  grid.draw_block(
    area.into(),
    crate::vt100::grid::BorderType::Plain,
    Attrs::default(),
  );
  grid.draw_text(
    Rect::new(area.x + 1, area.y, area.width - 2, 1),
    "Help",
    Attrs::default(),
  );

  let group = state.get_keymap_group();
  let items = match group {
    KeymapGroup::Procs => &[
      AppEvent::ToggleFocus,
      AppEvent::Quit,
      AppEvent::NextProc,
      AppEvent::PrevProc,
      AppEvent::StartProc,
      AppEvent::TermProc,
      AppEvent::RestartProc,
      AppEvent::ToggleKeymapWindow,
    ][..],
    KeymapGroup::Term => &[AppEvent::ToggleFocus][..],
    KeymapGroup::Copy => &[
      AppEvent::CopyModeEnd,
      AppEvent::CopyModeCopy,
      AppEvent::CopyModeLeave,
    ][..],
  };

  let area: crate::vt100::grid::Rect = area.into();
  let mut line = Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height,
  };
  for event in items {
    if let Some(key) = keymap.resolve_key(group, &event) {
      let a = Attrs::default();
      line.x = grid.draw_text(line, " <", a).right();
      line.x = grid
        .draw_text(line, &print_key(key), Attrs::default().fg(Color::YELLOW))
        .right();
      line.x = grid.draw_text(line, ": ", a).right();
      line.x = grid.draw_text(line, &event.desc(), a).right();
      line.x = grid.draw_text(line, "> ", a).right();
    }
  }
}
