use std::io;

use crossterm::{
  event::{Event, EventStream, KeyCode},
  execute,
  terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};
use futures::{future::FutureExt, select, StreamExt};
use tui::{backend::CrosstermBackend, Terminal};

use crate::{
  state::{Proc, State},
  ui_procs::render_procs,
};

type Term = Terminal<CrosstermBackend<io::Stdout>>;

enum LoopAction {
  Continue,
  Quit,
}

pub struct App {
  state: State,
}

impl App {
  pub fn new() -> Self {
    let state = State {
      procs: vec![
        Proc {
          name: "proc1".to_string(),
        },
        Proc {
          name: "proc2".to_string(),
        },
        Proc {
          name: "proc3".to_string(),
        },
        Proc {
          name: "proc1".to_string(),
        },
        Proc {
          name: "proc2".to_string(),
        },
        Proc {
          name: "proc3".to_string(),
        },
        Proc {
          name: "proc1".to_string(),
        },
        Proc {
          name: "proc2".to_string(),
        },
        Proc {
          name: "proc3".to_string(),
        },
        Proc {
          name: "proc1".to_string(),
        },
        Proc {
          name: "proc2".to_string(),
        },
        Proc {
          name: "proc3".to_string(),
        },
        Proc {
          name: "proc1".to_string(),
        },
        Proc {
          name: "proc2".to_string(),
        },
        Proc {
          name: "proc3".to_string(),
        },
      ],
      selected: 0,
    };
    App { state }
  }

  pub async fn run(self) -> Result<(), io::Error> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;

    let result = self.main_loop(&mut terminal).await;

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    result
  }

  async fn main_loop(mut self, terminal: &mut Term) -> Result<(), io::Error> {
    let mut input = EventStream::new();

    loop {
      terminal.draw(|f| {
        let area = f.size();

        render_procs(area, f, &mut self.state);
      })?;

      let loop_action = select! {
        event = input.next().fuse() => {
          self.handle_input(event)
        }
      };

      match loop_action {
        LoopAction::Continue => {}
        LoopAction::Quit => break,
      };
    }

    Ok(())
  }

  fn handle_input(
    &mut self,
    event: Option<crossterm::Result<Event>>,
  ) -> LoopAction {
    match event {
      Some(crossterm::Result::Ok(e)) => match e {
        Event::Key(key) => match key.code {
          crossterm::event::KeyCode::Char('q') => LoopAction::Quit,
          crossterm::event::KeyCode::Esc => LoopAction::Quit,

          KeyCode::Char('j') => {
            self.state.selected += 1;
            LoopAction::Continue
          }
          KeyCode::Char('k') => {
            self.state.selected -= 1;
            LoopAction::Continue
          }

          _ => LoopAction::Continue,
        },
        Event::Mouse(_) => LoopAction::Continue,
        Event::Resize(_, _) => LoopAction::Continue,
      },
      _ => LoopAction::Quit,
    }
  }
}
