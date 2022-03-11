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
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout},
  Terminal,
};

use crate::{
  proc::Proc, state::State, ui_procs::render_procs, ui_term::render_term,
};

type Term = Terminal<CrosstermBackend<io::Stdout>>;

enum LoopAction {
  Continue,
  Quit,
}

pub struct App {
  state: State,
  events: Receiver<()>,
  events_tx: Sender<()>,
}

impl App {
  pub fn new() -> Self {
    let (tx, rx) = channel::<()>(100);

    let state = State {
      procs: Vec::new(),
      selected: 0,
    };

    App {
      state,
      events: rx,
      events_tx: tx,
    }
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

    let mut cur_size: Option<(u16, u16)> = None;

    loop {
      let mut term_size = (0, 0);
      terminal.draw(|f| {
        let chunks = Layout::default()
          .direction(Direction::Horizontal)
          .constraints([Constraint::Length(30), Constraint::Min(2)].as_ref())
          .split(f.size());

        render_procs(chunks[0], f, &mut self.state);
        render_term(chunks[1], f, &mut self.state);

        term_size = (chunks[1].height - 2, chunks[1].width - 2);
      })?;

      match cur_size {
        Some((rows, cols)) => {
          if rows != term_size.0 || cols != term_size.1 {
            for proc in &self.state.procs {
              proc.inst.resize(term_size.0, term_size.1);
            }
          }
        }
        None => {
          cur_size = Some(term_size);
          self.start_procs(term_size);
        }
      }

      let loop_action = select! {
        event = input.next().fuse() => {
          self.handle_input(event)
        }
        _ = self.events.recv().fuse() => {
          LoopAction::Continue
        }
      };

      match loop_action {
        LoopAction::Continue => {}
        LoopAction::Quit => break,
      };
    }

    Ok(())
  }

  fn start_procs(&mut self, size: (u16, u16)) {
    self.state.procs.push(Proc::new(
      "proc1".to_string(),
      self.events_tx.clone(),
      size,
    ));
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
