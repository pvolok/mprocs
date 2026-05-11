use crate::color;
use crate::console::state::ConsoleState;
use crate::term::Grid;
use crate::term::attrs::Attrs;
use crate::term::grid::{BorderType, Pos, Rect};
use crate::term::key::Key;

pub struct ModalChoice {
  pub key: char,
  pub label: &'static str,
}

pub enum ModalAction {
  None,
  Detach,
  Quit,
}

pub trait Modal {
  fn title(&self) -> &str;
  fn size(&self) -> (u16, u16);
  fn draw_content(&self, grid: &mut Grid, area: Rect);
  fn handle_key(&mut self, key: Key, state: &mut ConsoleState) -> ModalAction;

  fn draw(&self, grid: &mut Grid) {
    let (width, height) = self.size();
    let area = grid.area();
    draw_backdrop(grid, area);

    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    let modal_rect = Rect::new(x, y, width, height);

    let border_color = color!("#bee6f4");
    let bg = color!("#1a1a2e");
    let border_attrs = Attrs::default().fg(border_color).bg(bg);
    grid.fill_area(modal_rect.inner(1), ' ', Attrs::default().bg(bg));
    grid.draw_block(modal_rect, &BorderType::Thick.chars(), border_attrs);

    let title_attrs = Attrs::default().fg(border_color).bg(bg).set_bold(true);
    let title_area = Rect::new(x + 1, y, width - 4, 1);
    grid.draw_text(title_area, &format!(" {} ", self.title()), title_attrs);

    let content_area = Rect::new(x + 2, y + 2, width - 4, height - 4);
    self.draw_content(grid, content_area);
  }
}

pub fn draw_choices(grid: &mut Grid, area: Rect, choices: &[ModalChoice]) {
  let bg = color!("#1a1a2e");
  for (i, choice) in choices.iter().enumerate() {
    let Some(row) = area.row(i as u16) else {
      break;
    };
    let key_attrs =
      Attrs::default().fg(color!("#7da8e8")).bg(bg).set_bold(true);
    let label_attrs = Attrs::default().fg(color!("#cccccc")).bg(bg);
    let used = grid.draw_text(row, &format!("{}", choice.key), key_attrs);
    let rest = Rect::new(used.right(), row.y, row.width - used.width, 1);
    grid.draw_text(rest, &format!(" - {}", choice.label), label_attrs);
  }
}

fn draw_backdrop(grid: &mut Grid, area: Rect) {
  let factor = 70;
  for row in area.y..area.y + area.height {
    for col in area.x..area.x + area.width {
      if let Some(cell) = grid.drawing_cell_mut(Pos { col, row }) {
        let mut attrs = *cell.attrs();
        attrs.fgcolor = attrs.fgcolor.dim(factor);
        attrs.bgcolor = attrs.bgcolor.dim(factor);
        cell.set_attrs(attrs);
      }
    }
  }
}
