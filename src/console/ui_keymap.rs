use crate::console::{
  keymap::{Keymap, KeymapGroup},
  state::State,
};
use crate::console::action::Action;
use crate::term::{attrs::Attrs, encode::print_key, grid::Rect, Color, Grid};

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
    &crate::term::grid::BorderType::Plain.chars(),
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
      Action::ToggleFocus,
      Action::Quit,
      Action::NextProc,
      Action::PrevProc,
      Action::StartProc,
      Action::TermProc,
      Action::RestartProc,
      Action::Zoom,
      Action::ShowCommandsMenu,
      Action::ToggleKeymapWindow,
    ][..],
    KeymapGroup::Term => &[Action::ToggleFocus][..],
    KeymapGroup::Copy => &[
      Action::CopyModeEnd,
      Action::CopyModeCopy,
      Action::CopyModeLeave,
    ][..],
  };

  let area: crate::term::grid::Rect = area.into();
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
