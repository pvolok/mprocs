use std::io;

use crossterm::{
  event::{Event, EventStream},
  execute,
  terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};
use futures::{
  future::{join_all, FutureExt},
  select, StreamExt,
};
use portable_pty::CommandBuilder;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout, Margin, Rect},
  Terminal,
};

use crate::{
  encode_term::{encode_key, KeyCodeEncodeModes},
  event::AppEvent,
  keymap::Keymap,
  proc::Proc,
  state::{Scope, State},
  ui_keymap::render_keymap,
  ui_procs::render_procs,
  ui_term::render_term,
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
      scope: Scope::Procs,
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

    {
      let area = AppLayout::new(terminal.size().unwrap()).term_area();
      let rows = area.height;
      let cols = area.width;
      self.start_procs((rows, cols));
    }

    loop {
      terminal.draw(|f| {
        let layout = AppLayout::new(f.size());

        render_procs(layout.procs, f, &mut self.state);
        render_term(layout.term, f, &mut self.state);
        render_keymap(layout.keymap, f, &mut self.state);
      })?;

      let keymap = Keymap::default();

      let loop_action = select! {
        event = input.next().fuse() => {
          self.handle_input(event, &keymap)
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

    join_all(self.state.procs.into_iter().map(|mut proc| {
      if proc.is_up() {
        proc.inst.killer.kill().unwrap();
      }
      proc.wait()
    }))
    .await;

    Ok(())
  }

  fn start_procs(&mut self, size: (u16, u16)) {
    self.state.procs.push(Proc::new(
      "zsh".to_string(),
      CommandBuilder::new("zsh"),
      self.events_tx.clone(),
      size,
    ));
    self.state.procs.push(Proc::new(
      "htop".to_string(),
      CommandBuilder::new("htop"),
      self.events_tx.clone(),
      size,
    ));
    self.state.procs.push(Proc::new(
      "top".to_string(),
      CommandBuilder::new("top"),
      self.events_tx.clone(),
      size,
    ));
    self.state.procs.push(Proc::new(
      "ls".to_string(),
      CommandBuilder::new("ls"),
      self.events_tx.clone(),
      size,
    ));
  }

  fn handle_input(
    &mut self,
    event: Option<crossterm::Result<Event>>,
    keymap: &Keymap,
  ) -> LoopAction {
    match event {
      Some(crossterm::Result::Ok(event)) => match event {
        Event::Key(key) => {
          if let Some(bound) = keymap.resolve(self.state.scope, &key) {
            self.handle_event(bound)
          } else if self.state.scope == Scope::Term {
            self.handle_event(&AppEvent::SendKey(key))
          } else {
            LoopAction::Continue
          }
        }
        Event::Mouse(_) => LoopAction::Continue,
        Event::Resize(width, height) => {
          let area = AppLayout::new(Rect::new(0, 0, width, height)).term_area();
          let rows = area.height;
          let cols = area.width;
          for proc in &self.state.procs {
            proc.inst.resize(rows, cols);
          }

          LoopAction::Continue
        }
      },
      _ => LoopAction::Quit,
    }
  }

  fn handle_event(&mut self, event: &AppEvent) -> LoopAction {
    match event {
      AppEvent::Quit => LoopAction::Quit,

      AppEvent::ToggleScope => {
        self.state.scope = self.state.scope.toggle();
        LoopAction::Continue
      }

      AppEvent::NextProc => {
        let mut next = self.state.selected + 1;
        if next >= self.state.procs.len() {
          next = 0;
        }
        self.state.selected = next;
        LoopAction::Continue
      }
      AppEvent::PrevProc => {
        let next = if self.state.selected > 0 {
          self.state.selected - 1
        } else {
          self.state.procs.len() - 1
        };
        self.state.selected = next;
        LoopAction::Continue
      }

      AppEvent::TermProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.term();
        }
        LoopAction::Continue
      }
      AppEvent::KillProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.kill();
        }
        LoopAction::Continue
      }

      AppEvent::SendKey(key) => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if proc.is_up() {
            let encoder = encode_key(
              key,
              KeyCodeEncodeModes {
                enable_csi_u_key_encoding: false,
                application_cursor_keys: false,
                newline_mode: false,
              },
            )
            .unwrap_or_else(|_| "?".to_owned());
            proc.inst.master.write_all(encoder.as_bytes()).unwrap();
          }
        }
        LoopAction::Continue
      }
    }
  }
}

struct AppLayout {
  procs: Rect,
  term: Rect,
  keymap: Rect,
}

impl AppLayout {
  pub fn new(area: Rect) -> Self {
    let top_bot = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Min(1), Constraint::Length(1)])
      .split(area);
    let chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Length(30), Constraint::Min(2)].as_ref())
      .split(top_bot[0]);

    Self {
      procs: chunks[0],
      term: chunks[1],
      keymap: top_bot[1],
    }
  }

  pub fn term_area(&self) -> Rect {
    self.term.inner(&Margin {
      vertical: 1,
      horizontal: 1,
    })
  }
}
