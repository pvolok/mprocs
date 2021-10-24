use crate::ctypes::types as cty;
use tui::layout::{Constraint, Direction, Layout, Rect};

#[no_mangle]
pub extern "C" fn tui_layout(
  spec: *const cty::Constraint,
  len: usize,
  dir: cty::Direction,
  area: cty::Rect,
  result: *mut cty::Rect,
) {
  let spec = unsafe { std::slice::from_raw_parts(spec, len) };

  let parts = Layout::default()
    .direction(match dir {
      cty::Direction::Horizontal => Direction::Horizontal,
      cty::Direction::Vertical => Direction::Vertical,
    })
    .constraints(
      spec
        .iter()
        .map(|c| Constraint::from(c.clone()))
        .collect::<Vec<Constraint>>(),
    )
    .split(Rect::from(area));

  let result = unsafe { std::slice::from_raw_parts_mut(result, len) };
  for (i, p) in parts.iter().enumerate() {
    let part = cty::Rect::from(*p);
    result[i] = part;
  }
}
