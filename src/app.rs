use anyhow::bail;
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEventKind};
use futures::{future::FutureExt, select};
use termwiz::escape::csi::CursorStyle;
use tokio::{
  io::AsyncReadExt,
  sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender},
};
use tui::{
  layout::{Constraint, Direction, Layout, Margin, Rect},
  Terminal,
};
use tui_input::Input;

use crate::{
  clipboard::copy,
  config::{CmdConfig, Config, ProcConfig, ServerConfig},
  error::ResultLogger,
  event::{AppEvent, CopyMove},
  key::Key,
  keymap::Keymap,
  proc::{CopyMode, Pos, Proc, ProcHandle, ProcState, ProcUpdate, StopSignal},
  protocol::{CltToSrv, ProxyBackend, SrvToClt},
  state::{Modal, Scope, State},
  ui_add_proc::render_input_dialog,
  ui_confirm_quit::render_confirm_quit,
  ui_keymap::render_keymap,
  ui_procs::{procs_check_hit, procs_get_clicked_index, render_procs},
  ui_remove_proc::render_remove_proc,
  ui_term::{render_term, term_check_hit},
  ui_zoom_tip::render_zoom_tip,
};

type Term = Terminal<ProxyBackend>;

#[derive(Debug, PartialEq)]
enum LoopAction {
  Render,
  Skip,
  ForceQuit,
}

impl Default for LoopAction {
  fn default() -> Self {
    LoopAction::Skip
  }
}

impl LoopAction {
  fn render(&mut self) {
    match self {
      LoopAction::Render => (),
      LoopAction::Skip => *self = LoopAction::Render,
      LoopAction::ForceQuit => (),
    }
  }

  fn force_quit(&mut self) {
    *self = LoopAction::ForceQuit;
  }
}

pub struct App {
  config: Config,
  keymap: Keymap,
  terminal: Term,
  state: State,
  client_rx: Receiver<CltToSrv>,
  client_tx: UnboundedSender<SrvToClt>,
  upd_rx: UnboundedReceiver<(usize, ProcUpdate)>,
  upd_tx: UnboundedSender<(usize, ProcUpdate)>,
  ev_rx: UnboundedReceiver<AppEvent>,
  ev_tx: UnboundedSender<AppEvent>,
}

impl App {
  pub async fn run(self) -> anyhow::Result<()> {
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

    result
  }

  async fn main_loop(mut self) -> anyhow::Result<()> {
    let mut last_term_size = {
      let area = self.get_layout().term_area();
      self.start_procs(area)?;
      (area.width, area.height)
    };

    let mut render_needed = true;
    let mut current_cursor_shape = CursorStyle::Default;
    loop {
      if render_needed {
        self.terminal.draw(|f| {
          let mut cursor_style = current_cursor_shape;

          let layout = AppLayout::new(
            f.size(),
            self.state.scope.is_zoomed(),
            &self.config,
          );

          {
            let term_area = layout.term_area();
            let term_size = (term_area.width, term_area.height);
            if last_term_size != term_size {
              last_term_size = term_size;
              for proc_handle in &mut self.state.procs {
                proc_handle.proc.resize(term_area);
              }
            }
          }

          render_procs(layout.procs, f, &mut self.state);
          render_term(layout.term, f, &mut self.state, &mut cursor_style);
          render_keymap(layout.keymap, f, &mut self.state, &self.keymap);
          render_zoom_tip(layout.zoom_banner, f, &self.keymap);

          if let Some(modal) = &mut self.state.modal {
            cursor_style = CursorStyle::Default;

            match modal {
              Modal::AddProc { input } => {
                render_input_dialog(f.size(), "Add process", f, input);
              }
              Modal::RenameProc { input } => {
                render_input_dialog(f.size(), "Rename process", f, input);
              }
              Modal::RemoveProc { id: _ } => {
                render_remove_proc(f.size(), f);
              }
              Modal::Quit => {
                render_confirm_quit(f.size(), f);
              }
            }
          }

          if current_cursor_shape != cursor_style {
            self
              .client_tx
              .send(SrvToClt::CursorShape(cursor_style))
              .log_ignore();
            current_cursor_shape = cursor_style;
          }
        })?;
      }

      let mut loop_action = LoopAction::default();
      let () = select! {
        event = self.client_rx.recv().fuse() => {
          if let Some(event) = event {
            self.handle_client_msg(&mut loop_action, event)?
          }
        }
        event = self.upd_rx.recv().fuse() => {
          if let Some(event) = event {
            self.handle_proc_update(&mut loop_action, event)
          }
        }
        event = self.ev_rx.recv().fuse() => {
          if let Some(event) = event {
            self.handle_event(&mut loop_action, &event)
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
      .map(|proc_cfg| ProcHandle {
        proc: Proc::new(
          proc_cfg.name.clone(),
          proc_cfg,
          self.upd_tx.clone(),
          size,
        ),
      })
      .collect::<Vec<_>>();

    self.state.procs.append(&mut procs);

    Ok(())
  }

  fn handle_client_msg(
    &mut self,
    loop_action: &mut LoopAction,
    msg: CltToSrv,
  ) -> anyhow::Result<()> {
    match msg {
      CltToSrv::Init { .. } => bail!("Init message is unexpected."),
      CltToSrv::Key(event) => Ok(self.handle_input(loop_action, event)),
    }
  }

  fn handle_input(&mut self, loop_action: &mut LoopAction, event: Event) {
    {
      let mut ret: bool = false;
      let mut reset_modal = false;
      if let Some(modal) = &mut self.state.modal {
        match modal {
          Modal::AddProc { input } => {
            match event {
              Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                ..
              }) if modifiers.is_empty() => {
                reset_modal = true;
                self
                  .ev_tx
                  .send(AppEvent::AddProc {
                    cmd: input.value().to_string(),
                  })
                  .unwrap();
                // Skip because AddProc event will immediately rerender.
                ret = true;
              }
              Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers,
                ..
              }) if modifiers.is_empty() => {
                reset_modal = true;
                loop_action.render();
                ret = true;
              }
              _ => (),
            }

            let req = tui_input::backend::crossterm::to_input_request(&event);
            if let Some(req) = req {
              input.handle(req);
              loop_action.render();
              ret = true;
            }
          }
          Modal::RenameProc { input } => {
            match event {
              Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                ..
              }) if modifiers.is_empty() => {
                reset_modal = true;
                self
                  .ev_tx
                  .send(AppEvent::RenameProc {
                    name: input.value().to_string(),
                  })
                  .unwrap();
                // Skip because RenameProc event will immediately rerender.
                ret = true;
              }
              Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers,
                ..
              }) if modifiers.is_empty() => {
                reset_modal = true;
                loop_action.render();
                ret = true;
              }
              _ => (),
            }

            let req = tui_input::backend::crossterm::to_input_request(&event);
            if let Some(req) = req {
              input.handle(req);
              loop_action.render();
              ret = true;
            }
          }
          Modal::RemoveProc { id } => {
            match event {
              Event::Key(KeyEvent {
                code: KeyCode::Char('y'),
                modifiers,
                ..
              }) if modifiers.is_empty() => {
                reset_modal = true;
                self.ev_tx.send(AppEvent::RemoveProc { id: *id }).unwrap();
                // Skip because RemoveProc event will immediately rerender.
                ret = true;
              }
              Event::Key(KeyEvent {
                code: KeyCode::Esc,
                modifiers,
                ..
              })
              | Event::Key(KeyEvent {
                code: KeyCode::Char('n'),
                modifiers,
                ..
              }) if modifiers.is_empty() => {
                reset_modal = true;
                loop_action.render();
                ret = true;
              }
              _ => (),
            }
          }
          Modal::Quit => match event {
            Event::Key(KeyEvent {
              code: KeyCode::Char('y'),
              modifiers,
              ..
            }) if modifiers.is_empty() => {
              reset_modal = true;
              self.ev_tx.send(AppEvent::Quit).unwrap();
              ret = true;
            }
            Event::Key(KeyEvent {
              code: KeyCode::Esc,
              modifiers,
              ..
            })
            | Event::Key(KeyEvent {
              code: KeyCode::Char('n'),
              modifiers,
              ..
            }) if modifiers.is_empty() => {
              reset_modal = true;
              loop_action.render();
              ret = true;
            }
            _ => (),
          },
        };
      }

      if reset_modal {
        self.state.modal = None;
      }
      if ret {
        return;
      }
    }

    match event {
      Event::Key(key) => {
        let key = Key::from(key);
        let group = self.state.get_keymap_group();
        if let Some(bound) = self.keymap.resolve(group, &key) {
          let bound = bound.clone();
          self.handle_event(loop_action, &bound)
        } else {
          match self.state.scope {
            Scope::Procs => (),
            Scope::Term | Scope::TermZoom => {
              self.handle_event(loop_action, &AppEvent::SendKey { key })
            }
          }
        }
      }
      Event::Mouse(mev) => {
        if mev.kind == MouseEventKind::Moved {
          return;
        }

        let layout = self.get_layout();
        if term_check_hit(layout.term_area(), mev.column, mev.row) {
          match (self.state.scope, mev.kind) {
            (Scope::Procs, MouseEventKind::Down(_)) => {
              self.state.scope = Scope::Term
            }
            _ => (),
          }
          if let Some(proc) = self.state.get_current_proc_mut() {
            proc.handle_mouse(mev, layout.term_area(), &self.config);
          }
        } else if procs_check_hit(layout.procs, mev.column, mev.row) {
          match (self.state.scope, mev.kind) {
            (Scope::Term, MouseEventKind::Down(_)) => {
              self.state.scope = Scope::Procs
            }
            _ => (),
          }
          match mev.kind {
            MouseEventKind::Down(btn) => match btn {
              MouseButton::Left => {
                if let Some(index) = procs_get_clicked_index(
                  layout.procs,
                  mev.column,
                  mev.row,
                  &self.state,
                ) {
                  self.state.select_proc(index);
                }
              }
              MouseButton::Right | MouseButton::Middle => (),
            },
            MouseEventKind::Up(_) => (),
            MouseEventKind::Drag(_) => (),
            MouseEventKind::Moved => (),
            MouseEventKind::ScrollDown => {
              if self.state.selected < self.state.procs.len().saturating_sub(1)
              {
                let index = self.state.selected + 1;
                self.state.select_proc(index);
              }
            }
            MouseEventKind::ScrollUp => {
              if self.state.selected > 0 {
                let index = self.state.selected - 1;
                self.state.select_proc(index);
              }
            }
            MouseEventKind::ScrollLeft => (),
            MouseEventKind::ScrollRight => (),
          }
        }
        loop_action.render();
      }
      Event::Resize(width, height) => {
        let area = AppLayout::new(
          Rect::new(0, 0, width, height),
          self.state.scope.is_zoomed(),
          &self.config,
        )
        .term_area();
        for proc_handle in &mut self.state.procs {
          proc_handle.proc.resize(area);
        }

        self.terminal.backend_mut().set_size(width, height);
        self
          .terminal
          .resize(Rect {
            x: 0,
            y: 0,
            width,
            height,
          })
          .log_ignore();

        loop_action.render();
      }
      Event::FocusGained => {
        log::warn!("Ignore input event: {:?}", event);
      }
      Event::FocusLost => {
        log::warn!("Ignore input event: {:?}", event);
      }
      Event::Paste(_) => {
        log::warn!("Ignore input event: {:?}", event);
      }
    }
  }

  fn handle_event(&mut self, loop_action: &mut LoopAction, event: &AppEvent) {
    match event {
      AppEvent::Batch { cmds } => {
        for cmd in cmds {
          self.handle_event(loop_action, cmd);
          if *loop_action == LoopAction::ForceQuit {
            return;
          }
        }
      }

      AppEvent::QuitOrAsk => {
        let have_running = self.state.procs.iter().any(|p| p.proc.is_up());
        if have_running {
          self.state.modal = Some(Modal::Quit);
        } else {
          self.state.quitting = true;
        }
        loop_action.render();
      }
      AppEvent::Quit => {
        self.state.quitting = true;
        for proc_handle in self.state.procs.iter_mut() {
          if proc_handle.proc.is_up() {
            proc_handle.proc.stop();
          }
        }
        loop_action.render();
      }
      AppEvent::ForceQuit => {
        for proc_handle in self.state.procs.iter_mut() {
          if proc_handle.proc.is_up() {
            proc_handle.proc.kill();
          }
        }
        loop_action.force_quit();
      }

      AppEvent::ToggleFocus => {
        self.state.scope = self.state.scope.toggle();
        loop_action.render();
      }
      AppEvent::FocusProcs => {
        self.state.scope = Scope::Procs;
        loop_action.render();
      }
      AppEvent::FocusTerm => {
        self.state.scope = Scope::Term;
        loop_action.render();
      }
      AppEvent::Zoom => {
        self.state.scope = Scope::TermZoom;
        loop_action.render();
      }

      AppEvent::NextProc => {
        let mut next = self.state.selected + 1;
        if next >= self.state.procs.len() {
          next = 0;
        }
        self.state.select_proc(next);
        loop_action.render();
      }
      AppEvent::PrevProc => {
        let next = if self.state.selected > 0 {
          self.state.selected - 1
        } else {
          self.state.procs.len() - 1
        };
        self.state.select_proc(next);
        loop_action.render();
      }
      AppEvent::SelectProc { index } => {
        self.state.select_proc(*index);
        loop_action.render();
      }

      AppEvent::StartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.start();
        }
      }
      AppEvent::TermProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.stop();
        }
      }
      AppEvent::KillProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.kill();
        }
      }
      AppEvent::RestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if proc.is_up() {
            proc.stop();
            proc.to_restart = true;
          } else {
            proc.start();
          }
        }
      }
      AppEvent::ForceRestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if proc.is_up() {
            proc.kill();
            proc.to_restart = true;
          } else {
            proc.start();
          }
        }
      }

      AppEvent::ScrollUpLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.scroll_up_lines(*n);
          loop_action.render();
        }
      }
      AppEvent::ScrollDownLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.scroll_down_lines(*n);
          loop_action.render();
        }
      }
      AppEvent::ScrollUp => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.scroll_half_screen_up();
          loop_action.render();
        }
      }
      AppEvent::ScrollDown => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.scroll_half_screen_down();
          loop_action.render();
        }
      }
      AppEvent::ShowAddProc => {
        self.state.modal = Some(Modal::AddProc {
          input: Input::default(),
        });
        loop_action.render();
      }
      AppEvent::AddProc { cmd } => {
        let proc = Proc::new(
          cmd.to_string(),
          &ProcConfig {
            name: cmd.to_string(),
            cmd: CmdConfig::Shell {
              shell: cmd.to_string(),
            },
            cwd: None,
            env: None,
            autostart: true,
            stop: StopSignal::default(),
          },
          self.upd_tx.clone(),
          self.get_layout().term_area(),
        );
        let proc_handle = ProcHandle { proc };
        self.state.procs.push(proc_handle);
        loop_action.render();
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
            loop_action.render();
          }
          None => (),
        }
      }
      AppEvent::RemoveProc { id } => {
        self
          .state
          .procs
          .retain(|p| p.proc.is_up() || p.proc.id != *id);
        loop_action.render();
      }

      AppEvent::ShowRenameProc => {
        self.state.modal = Some(Modal::RenameProc {
          input: Input::default(),
        });
        loop_action.render();
      }
      AppEvent::RenameProc { name } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.rename(name);
          loop_action.render();
        }
      }

      AppEvent::CopyModeEnter => {
        let switched = match self.state.get_current_proc_mut() {
          Some(proc) => match &mut proc.inst {
            ProcState::None => false,
            ProcState::Some(inst) => {
              let screen = inst.vt.read().unwrap().screen().clone();
              let y = (screen.size().0 - 1) as i32;
              proc.copy_mode = CopyMode::Start(screen, Pos { y, x: 0 });
              true
            }
            ProcState::Error(_) => false,
          },
          None => false,
        };
        if switched {
          self.state.scope = Scope::Term;
        }
        loop_action.render();
      }
      AppEvent::CopyModeLeave => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.copy_mode = CopyMode::None(None);
        }
        loop_action.render();
      }
      AppEvent::CopyModeMove { dir } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &proc.inst {
            ProcState::None => (),
            ProcState::Some(inst) => {
              let vt = inst.vt.read().unwrap();
              let screen = vt.screen();
              match &mut proc.copy_mode {
                CopyMode::None(_) => (),
                CopyMode::Start(_, pos_) | CopyMode::Range(_, _, pos_) => {
                  match dir {
                    CopyMove::Up => {
                      if pos_.y > -(screen.scrollback_len() as i32) {
                        pos_.y -= 1
                      }
                    }
                    CopyMove::Right => {
                      if pos_.x + 1 < screen.size().1 as i32 {
                        pos_.x += 1
                      }
                    }
                    CopyMove::Left => {
                      if pos_.x > 0 {
                        pos_.x -= 1
                      }
                    }
                    CopyMove::Down => {
                      if pos_.y + 1 < screen.size().0 as i32 {
                        pos_.y += 1
                      }
                    }
                  };
                }
              }
            }
            ProcState::Error(_) => (),
          }
        }
        loop_action.render();
      }
      AppEvent::CopyModeEnd => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.copy_mode = match std::mem::take(&mut proc.copy_mode) {
            CopyMode::Start(screen, start) => {
              CopyMode::Range(screen, start.clone(), start)
            }
            other => other,
          };
        }
        loop_action.render();
      }
      AppEvent::CopyModeCopy => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if let CopyMode::Range(screen, start, end) = &proc.copy_mode {
            let (low, high) = Pos::to_low_high(start, end);
            let text = screen.get_selected_text(low.x, low.y, high.x, high.y);

            copy(text.as_str());
          }
          proc.copy_mode = CopyMode::None(None);
        }
        loop_action.render();
      }

      AppEvent::SendKey { key } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send_key(key);
        }
      }
    }
  }

  fn handle_proc_update(
    &mut self,
    loop_action: &mut LoopAction,
    event: (usize, ProcUpdate),
  ) {
    match event.1 {
      ProcUpdate::Render => {
        let cur_proc_id =
          self.state.get_current_proc().map_or(usize::MAX, |p| p.id);
        if let Some(proc) = self.state.get_proc_mut(event.0) {
          if proc.id != cur_proc_id {
            proc.changed = true;
          }
          loop_action.render();
        }
      }
      ProcUpdate::Stopped => {
        if let Some(proc) = self.state.get_proc_mut(event.0) {
          if proc.to_restart {
            proc.start();
            proc.to_restart = false;
          }
        }
        loop_action.render();
      }
      ProcUpdate::Started => {
        loop_action.render();
      }
    }
  }

  fn get_layout(&mut self) -> AppLayout {
    AppLayout::new(
      self.terminal.get_frame().size(),
      self.state.scope.is_zoomed(),
      &self.config,
    )
  }
}

struct AppLayout {
  procs: Rect,
  term: Rect,
  keymap: Rect,
  zoom_banner: Rect,
}

impl AppLayout {
  pub fn new(area: Rect, zoom: bool, config: &Config) -> Self {
    let keymap_h = if zoom || config.hide_keymap_window {
      0
    } else {
      3
    };
    let procs_w = if zoom {
      0
    } else {
      config.proc_list_width as u16
    };
    let zoom_banner_h = if zoom { 1 } else { 0 };
    let top_bot = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Min(1), Constraint::Length(keymap_h)])
      .split(area);
    let chunks = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Length(procs_w), Constraint::Min(2)].as_ref())
      .split(top_bot[0]);
    let term_zoom = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Length(zoom_banner_h), Constraint::Min(1)])
      .split(chunks[1]);

    Self {
      procs: chunks[0],
      term: term_zoom[1],
      keymap: top_bot[1],
      zoom_banner: term_zoom[0],
    }
  }

  pub fn term_area(&self) -> Rect {
    self.term.inner(&Margin {
      vertical: 1,
      horizontal: 1,
    })
  }
}

pub async fn server_main(
  config: Config,
  keymap: Keymap,
  client_tx: tokio::sync::mpsc::UnboundedSender<SrvToClt>,
  mut client_rx: tokio::sync::mpsc::Receiver<CltToSrv>,
) -> anyhow::Result<()> {
  let init = client_rx
    .recv()
    .await
    .ok_or_else(|| anyhow::Error::msg("Expected init message."))?;
  let backend = match init {
    CltToSrv::Init { width, height } => {
      let proxy_backend = ProxyBackend {
        tx: client_tx.clone(),
        width,
        height,
      };
      proxy_backend
    }
    _ => bail!("Expected init message."),
  };

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
    keymap,
    terminal,
    state,
    client_rx,
    client_tx,
    upd_rx,
    upd_tx,

    ev_rx,
    ev_tx,
  };
  let client_tx = app.client_tx.clone();
  app.run().await?;
  client_tx.send(SrvToClt::Quit).unwrap();

  Ok(())
}
