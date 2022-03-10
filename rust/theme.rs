use tui::style::{Color, Modifier, Style};

#[derive(Default)]
pub struct Theme;

impl Theme {
  pub fn pane_frame(self, active: bool) -> Style {
    let style = Style::default();
    if active {
      style.fg(Color::Reset).add_modifier(Modifier::BOLD)
    } else {
      style.fg(Color::Rgb(128, 128, 128))
    }
  }
}
