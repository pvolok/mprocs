use tui::{
  style::{Color, Modifier, Style},
  widgets::{Block, BorderType, Borders},
};

pub struct Theme {
  pub procs_item: Style,
  pub procs_item_active: Style,
}

impl Theme {
  pub fn pane_title(&self, active: bool) -> Style {
    let style = Style::default();
    if active {
      style.fg(Color::Reset).add_modifier(Modifier::BOLD)
    } else {
      style.fg(Color::Reset)
    }
  }

  pub fn pane(&self, active: bool) -> Block {
    let type_ = match active {
      true => BorderType::Thick,
      false => BorderType::Plain,
    };

    Block::default()
      .borders(Borders::ALL)
      .border_type(type_)
      .border_style(Style::default().fg(Color::Reset).bg(Color::Reset))
  }

  pub fn get_procs_item(&self, active: bool) -> Style {
    if active {
      self.procs_item_active
    } else {
      self.procs_item
    }
  }

  pub fn zoom_tip(&self) -> Style {
    Style::default().fg(Color::Black).bg(Color::Yellow)
  }
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      procs_item: Style::default().fg(Color::Reset),
      procs_item_active: Style::default().bg(Color::Indexed(240)),
    }
  }
}
