use std::io;

use crossterm::{
  event::{Event, EventStream, KeyCode, KeyEvent},
  execute,
  terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
    LeaveAlternateScreen,
  },
};
use futures::{future::FutureExt, select, StreamExt};
use portable_pty::CommandBuilder;
use tokio::{
  io::AsyncReadExt,
  sync::mpsc::{UnboundedReceiver, UnboundedSender},
};
use tui::{
  backend::CrosstermBackend,
  layout::{Constraint, Direction, Layout, Margin, Rect},
  Terminal,
};
use tui_input::Input;

use crate::{
  config::{CmdConfig, Config, ProcConfig, ServerConfig},
  encode_term::{encode_key, KeyCodeEncodeModes},
  event::AppEvent,
  key::Key,
  keymap::Keymap,
  proc::{Proc, ProcState, ProcUpdate},
  state::{Modal, Scope, State},
  ui_add_proc::render_add_proc,
  ui_keymap::render_keymap,
  ui_procs::render_procs,
  ui_remove_proc::render_remove_proc,
  ui_term::render_term,
};

type Term = Terminal<CrosstermBackend<io::Stdout>>;

enum LoopAction {
  Render,
  Skip,
  ForceQuit,
}

pub struct App {
  config: Config,
  terminal: Term,
  state: State,
  upd_rx: UnboundedReceiver<(usize, ProcUpdate)>,
  upd_tx: UnboundedSender<(usize, ProcUpdate)>,
  ev_rx: UnboundedReceiver<AppEvent>,
  ev_tx: UnboundedSender<AppEvent>,
}

impl App {
  pub fn from_config_file(config: Config) -> anyhow::Result<Self> {
    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend)?;

    let (upd_tx, upd_rx) =
      tokio::sync::mpsc::unbounded_channel::<(usize, ProcUpdate)>();
    let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();

    let state = State {
      scope: Scope::Procs,
      procs: Vec::new(),
      selected: 0,

      modal: None,

      quitting: false,
    };

    let app = App {
      config,
      terminal,
      state,
      upd_rx,
      upd_tx,

      ev_rx,
      ev_tx,
    };
    Ok(app)
  }

  pub async fn run(self) -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;

    let (exit_trigger, exit_listener) = triggered::trigger();

    let server_thread = if let Some(ref server_addr) = self.config.server {
      let server = match server_addr {
        ServerConfig::Tcp(addr) => tokio::net::TcpListener::bind(addr).await?,
      };

      let ev_tx = self.ev_tx.clone();
      let server_thread = tokio::spawn(async move {
        loop {
          let on_exit = exit_listener.clone();
          let mut socket: tokio::net::TcpStream = select! {
            _ = on_exit.fuse() => break,
            client = server.accept().fuse() => {
              if let Ok((socket, _)) = client {
                socket
              } else {
                break;
              }
            }
          };

          let ctl_tx = ev_tx.clone();
          let on_exit = exit_listener.clone();
          tokio::spawn(async move {
            let mut buf: Vec<u8> = Vec::with_capacity(32);
            let () = select! {
              _ = on_exit.fuse() => return,
              count = socket.read_to_end(&mut buf).fuse() => {
                if count.is_err() {
                  return;
                }
              }
            };
            let msg: AppEvent = serde_yaml::from_slice(buf.as_slice()).unwrap();
            // log::info!("Received remote command: {:?}", msg);
            ctl_tx.send(msg).unwrap();
          });
        }
      });
      Some(server_thread)
    } else {
      None
    };

    let result = self.main_loop().await;

    exit_trigger.trigger();
    if let Some(server_thread) = server_thread {
      let _ = server_thread.await;
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    result
  }

  async fn main_loop(mut self) -> anyhow::Result<()> {
    let mut input = EventStream::new();

    {
      let area = AppLayout::new(self.terminal.size().unwrap()).term_area();
      self.start_procs(area)?;
    }

    let mut render_needed = true;
    loop {
      if render_needed {
        self.terminal.draw(|f| {
          let layout = AppLayout::new(f.size());

          render_procs(layout.procs, f, &mut self.state);
          render_term(layout.term, f, &mut self.state);
          render_keymap(layout.keymap, f, &mut self.state);

          if let Some(modal) = &mut self.state.modal {
            match modal {
              Modal::AddProc { input } => {
                render_add_proc(f.size(), f, input);
              }
              Modal::RemoveProc { id: _ } => {
                render_remove_proc(f.size(), f);
              }
            }
          }
        })?;
      }

      let keymap = Keymap::default();

      let loop_action = select! {
        event = input.next().fuse() => {
          self.handle_input(event, &keymap)
        }
        event = self.upd_rx.recv().fuse() => {
          if let Some(event) = event {
            self.handle_proc_update(event)
          } else {
            LoopAction::Skip
          }
        }
        event = self.ev_rx.recv().fuse() => {
          if let Some(event) = event {
            self.handle_event(&event)
          } else {
            LoopAction::Skip
          }
        }
      };

      if self.state.quitting && self.state.all_procs_down() {
        break;
      }

      match loop_action {
        LoopAction::Render => {
          render_needed = true;
        }
        LoopAction::Skip => {
          render_needed = false;
        }
        LoopAction::ForceQuit => break,
      };
    }

    Ok(())
  }

  fn start_procs(&mut self, size: Rect) -> anyhow::Result<()> {
    let mut procs = self
      .config
      .procs
      .iter()
      .map(|proc_cfg| {
        let cmd = CommandBuilder::from(proc_cfg);

        Proc::new(proc_cfg.name.clone(), cmd, self.upd_tx.clone(), size)
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
    let event = match event {
      Some(Ok(event)) => event,
      Some(Err(err)) => {
        log::warn!("Crossterm input error: {}", err.to_string());
        return LoopAction::Skip;
      }
      None => {
        log::warn!("Crossterm input is None.");
        return LoopAction::Skip;
      }
    };

    {
      let mut ret: Option<LoopAction> = None;
      let mut reset_modal = false;
      if let Some(modal) = &mut self.state.modal {
        match modal {
          Modal::AddProc { input } => {
            match event {
              Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
              }) if modifiers.is_empty() => {
                reset_modal = true;
                self
                  .ev_tx
                  .send(AppEvent::AddProc {
                    cmd: input.value().to_string(),
                  })
                  .unwrap();
                // Skip because AddProc event will immediately rerender.
                ret = Some(LoopAction::Skip);
              }
              Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers,
              }) if modifiers.is_empty() => {
                reset_modal = true;
                ret = Some(LoopAction::Render);
              }
              _ => (),
            }

            let req = tui_input::backend::crossterm::to_input_request(event);
            if let Some(req) = req {
              input.handle(req);
              ret = Some(LoopAction::Render);
            }
          }
          Modal::RemoveProc { id } => {
            match event {
              Event::Key(KeyEvent {
                code: KeyCode::Char('y'),
                modifiers,
              }) if modifiers.is_empty() => {
                reset_modal = true;
                self.ev_tx.send(AppEvent::RemoveProc { id: *id }).unwrap();
                // Skip because RemoveProc event will immediately rerender.
                ret = Some(LoopAction::Skip);
              }
              Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers,
              })
              | Event::Key(KeyEvent {
                code: KeyCode::Char('n'),
                modifiers,
              }) if modifiers.is_empty() => {
                reset_modal = true;
                ret = Some(LoopAction::Render);
              }
              _ => (),
            }
          }
        };
      }

      if reset_modal {
        self.state.modal = None;
      }
      if let Some(ret) = ret {
        return ret;
      }
    }

    match event {
      Event::Key(key) => {
        let key = Key::from(key);
        if let Some(bound) = keymap.resolve(self.state.scope, &key) {
          self.handle_event(bound)
        } else if self.state.scope == Scope::Term {
          self.handle_event(&AppEvent::SendKey { key })
        } else {
          LoopAction::Skip
        }
      }
      Event::Mouse(_) => LoopAction::Skip,
      Event::Resize(width, height) => {
        let (width, height) = if cfg!(windows) {
          crossterm::terminal::size().unwrap()
        } else {
          (width, height)
        };

        let area = AppLayout::new(Rect::new(0, 0, width, height)).term_area();
        for proc in &mut self.state.procs {
          proc.resize(area);
        }

        LoopAction::Render
      }
    }
  }

  fn handle_event(&mut self, event: &AppEvent) -> LoopAction {
    match event {
      AppEvent::Quit => {
        self.state.quitting = true;
        for proc in self.state.procs.iter_mut() {
          if proc.is_up() {
            proc.term();
          }
        }
        LoopAction::Render
      }
      AppEvent::ForceQuit => {
        for proc in self.state.procs.iter_mut() {
          if proc.is_up() {
            proc.kill();
          }
        }
        LoopAction::ForceQuit
      }

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
      AppEvent::RestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.term();
          proc.to_restart = true;
        }
        LoopAction::Skip
      }
      AppEvent::ForceRestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.kill();
          proc.to_restart = true;
        }
        LoopAction::Skip
      }

      AppEvent::ScrollUp => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.scroll_up();
          return LoopAction::Render;
        }
        LoopAction::Skip
      }
      AppEvent::ScrollDown => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.scroll_down();
          return LoopAction::Render;
        }
        LoopAction::Skip
      }
      AppEvent::ShowAddProc => {
        self.state.modal = Some(Modal::AddProc {
          input: Input::default(),
        });
        LoopAction::Render
      }
      AppEvent::AddProc { cmd } => {
        let proc = Proc::new(
          cmd.to_string(),
          (&ProcConfig {
            name: cmd.to_string(),
            cmd: CmdConfig::Shell {
              shell: cmd.to_string(),
            },
            cwd: None,
            env: None,
          })
            .into(),
          self.upd_tx.clone(),
          self.terminal.size().unwrap(),
        );
        self.state.procs.push(proc);
        LoopAction::Render
      }
      AppEvent::ShowRemoveProc => {
        let id = self
          .state
          .get_current_proc()
          .map(|proc| if proc.is_up() { None } else { Some(proc.id) })
          .flatten();
        match id {
          Some(id) => {
            self.state.modal = Some(Modal::RemoveProc { id });
            LoopAction::Render
          }
          None => LoopAction::Skip,
        }
      }
      AppEvent::RemoveProc { id } => {
        self
          .state
          .procs
          .retain(|proc| proc.is_up() || proc.id != *id);
        LoopAction::Render
      }

      AppEvent::SendKey { key } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if proc.is_up() {
            let application_cursor_keys = match &proc.inst {
              ProcState::None => unreachable!(),
              ProcState::Some(inst) => {
                inst.vt.read().unwrap().screen().application_cursor()
              }
              ProcState::Error(_) => unreachable!(),
            };
            let encoder = encode_key(
              key,
              KeyCodeEncodeModes {
                enable_csi_u_key_encoding: false,
                application_cursor_keys,
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
      ProcUpdate::Stopped => {
        if let Some(proc) = self.state.get_proc_mut(event.0) {
          if proc.to_restart {
            proc.start();
            proc.to_restart = false;
          }
        }
        LoopAction::Render
      }
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
