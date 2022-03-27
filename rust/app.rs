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
  config::Config,
  encode_term::{encode_key, KeyCodeEncodeModes},
  event::AppEvent,
  keymap::Keymap,
  proc::{Proc, ProcUpdate},
  state::{Scope, State},
  ui_keymap::render_keymap,
  ui_procs::render_procs,
  ui_term::render_term,
};

type Term = Terminal<CrosstermBackend<io::Stdout>>;

enum LoopAction {
  Render,
  Skip,
  Quit,
}

pub struct App {
  state: State,
  events: Receiver<(usize, ProcUpdate)>,
  events_tx: Sender<(usize, ProcUpdate)>,
}

impl App {
  pub fn new() -> Self {
    let (tx, rx) = channel::<(usize, ProcUpdate)>(100);

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

  pub async fn run(self) -> anyhow::Result<()> {
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

  async fn main_loop(mut self, terminal: &mut Term) -> anyhow::Result<()> {
    let mut input = EventStream::new();

    {
      let area = AppLayout::new(terminal.size().unwrap()).term_area();
      self.start_procs(area)?;
    }

    let mut render_needed = true;
    loop {
      if render_needed {
        terminal.draw(|f| {
          let layout = AppLayout::new(f.size());

          render_procs(layout.procs, f, &mut self.state);
          render_term(layout.term, f, &mut self.state);
          render_keymap(layout.keymap, f, &mut self.state);
        })?;
      }

      let keymap = Keymap::default();

      let loop_action = select! {
        event = input.next().fuse() => {
          self.handle_input(event, &keymap)
        }
        event = self.events.recv().fuse() => {
          if let Some(event) = event {
            self.handle_proc_update(event)
          } else {
            LoopAction::Skip
          }
        }
      };

      match loop_action {
        LoopAction::Render => {
          render_needed = true;
        }
        LoopAction::Skip => {
          render_needed = false;
        }
        LoopAction::Quit => break,
      };
    }

    join_all(self.state.procs.into_iter().map(|mut proc| {
      if proc.is_up() {
        if let Some(ref mut inst) = proc.inst {
          inst.killer.kill().unwrap();
        }
      }
      proc.wait()
    }))
    .await;

    Ok(())
  }

  fn start_procs(&mut self, size: Rect) -> anyhow::Result<()> {
    let config = Config::from_file("mprocs.json")?;

    let mut procs = config
      .procs
      .into_iter()
      .enumerate()
      .map(|(id, (name, proc_cfg))| {
        let cmd = CommandBuilder::from(proc_cfg);

        Proc::new(id, name, cmd, self.events_tx.clone(), size)
      })
      .collect::<Vec<_>>();

    self.state.procs.append(&mut procs);

    Ok(())
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
            LoopAction::Skip
          }
        }
        Event::Mouse(_) => LoopAction::Skip,
        Event::Resize(width, height) => {
          let area = AppLayout::new(Rect::new(0, 0, width, height)).term_area();
          for proc in &mut self.state.procs {
            proc.resize(area);
          }

          LoopAction::Render
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
        LoopAction::Render
      }

      AppEvent::NextProc => {
        let mut next = self.state.selected + 1;
        if next >= self.state.procs.len() {
          next = 0;
        }
        self.state.selected = next;
        LoopAction::Render
      }
      AppEvent::PrevProc => {
        let next = if self.state.selected > 0 {
          self.state.selected - 1
        } else {
          self.state.procs.len() - 1
        };
        self.state.selected = next;
        LoopAction::Render
      }

      AppEvent::StartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.start();
        }
        LoopAction::Skip
      }
      AppEvent::TermProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.term();
        }
        LoopAction::Skip
      }
      AppEvent::KillProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.kill();
        }
        LoopAction::Skip
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
            proc.write_all(encoder.as_bytes());
          }
        }
        LoopAction::Skip
      }
    }
  }

  fn handle_proc_update(&mut self, event: (usize, ProcUpdate)) -> LoopAction {
    match event.1 {
      ProcUpdate::Render => {
        if let Some(proc) = self.state.get_current_proc().as_ref() {
          if proc.id == event.0 {
            return LoopAction::Render;
          }
        }
        LoopAction::Skip
      }
      ProcUpdate::Stopped => LoopAction::Render,
      ProcUpdate::Started => LoopAction::Render,
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
      .constraints([Constraint::Min(1), Constraint::Length(3)])
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
