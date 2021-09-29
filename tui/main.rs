//mod event;
//mod events;
//mod layout;
//mod render;
//mod render_widget;
//mod style;
//mod terminal;

use std::{cell::RefCell, io};

use crossterm::{
  terminal::{EnterAlternateScreen, LeaveAlternateScreen},
  ExecutableCommand,
};
use crossterm::event::{read, Event};
//use ocaml::{Error, Pointer};
use tui::{backend::CrosstermBackend, Terminal};

use tui::widgets::{Widget, Block, Borders};
use tui::layout::{Layout, Constraint, Direction};

pub fn main() -> Result<(), io::Error> {
  let stdout = io::stdout();
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  let mut i = 1;

  loop {
      terminal.draw(|f| {
        let size = f.size();
        let block = Block::default()
          .title(i.to_string())
          .borders(Borders::ALL);
        f.render_widget(block, size);
      })?;

      read().unwrap();
      if i > 10 {
          break;
      } else {
          i += 1;
      }
  };

  //std::thread::sleep(std::time::Duration::seconds(2));

  Ok(())
}
