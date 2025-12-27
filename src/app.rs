use std::{any::Any, collections::HashMap, fmt::Debug, time::Instant};

use anyhow::bail;
use crossterm::event::{
  Event, KeyEvent, KeyEventKind, MouseButton, MouseEventKind,
};
use futures::{future::FutureExt, select};
use serde::{Deserialize, Serialize};
use termwiz::escape::csi::CursorStyle;
use tokio::{io::AsyncReadExt, sync::mpsc::UnboundedReceiver};
use tui::{
  layout::{Constraint, Direction, Layout, Margin, Rect},
  widgets::Widget,
  Terminal,
};

use crate::{
  config::{CmdConfig, Config, ProcConfig, ServerConfig},
  error::ResultLogger,
  event::{AppEvent, CopyMove},
  host::{receiver::MsgReceiver, sender::MsgSender},
  kernel::{
    kernel_message::{KernelCommand, ProcContext, ProcSender},
    proc::{ProcId, ProcInit, ProcStatus},
  },
  key::Key,
  keymap::Keymap,
  modal::{
    add_proc::AddProcModal, commands_menu::CommandsMenuModal,
    modal::Modal, quit::QuitModal,
    remove_proc::RemoveProcModal, rename_proc::RenameProcModal,
  },
  mouse::MouseEvent,
  proc::{
    msg::{ProcCmd, ProcUpdate},
    proc::launch_proc,
    view::{TargetState, RESTART_THRESHOLD_SECONDS},
    CopyMode, Pos, StopSignal,
  },
  protocol::{CltToSrv, ProxyBackend, SrvToClt},
  server::server_message::ServerMessage,
  state::{Scope, State},
  ui_keymap::render_keymap,
  ui_procs::{procs_check_hit, procs_get_clicked_index, render_procs},
  ui_term::{render_term, term_check_hit},
  ui_zoom_tip::render_zoom_tip,
  vt100::{MouseProtocolMode, Size},
};

type Term = Terminal<ProxyBackend>;

#[derive(Debug, Default, PartialEq)]
pub enum LoopAction {
  Render,
  #[default]
  Skip,
  ForceQuit,
}

impl LoopAction {
  pub fn render(&mut self) {
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
  state: State,
  modal: Option<Box<dyn Modal>>,
  pr: tokio::sync::mpsc::UnboundedReceiver<ProcCmd>,
  pc: ProcContext,

  screen_size: Size,
  clients: Vec<ClientHandle>,
}

impl App {
  pub async fn run(self) -> anyhow::Result<()> {
    let (exit_trigger, exit_listener) = triggered::trigger();

    let app_proc_id = self.pc.proc_id;
    let server_thread = if let Some(ref server_addr) = self.config.server {
      let server = match server_addr {
        ServerConfig::Tcp(addr) => tokio::net::TcpListener::bind(addr).await?,
      };

      let ev_pc = self.pc.clone();
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

          let ctl_pc = ev_pc.clone();
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
            ctl_pc.send(KernelCommand::ProcCmd(
              app_proc_id,
              ProcCmd::Custom(Box::new(msg)),
            ));
          });
        }
      });
      Some(server_thread)
    } else {
      None
    };

    let result = self.main_loop().await;
    exit_trigger.trigger();
    if let Err(err) = result {
      log::error!("App main loop error: {err}");
    }

    if let Some(server_thread) = server_thread {
      let _ = server_thread.await;
    }

    Ok(())
  }

  async fn main_loop(mut self) -> anyhow::Result<()> {
    self.pc.send(KernelCommand::ListenProcUpdates);

    self.start_procs(Rect::new(
      0,
      0,
      self.screen_size.width,
      self.screen_size.height,
    ))?;

    let mut render_needed = true;
    let mut last_term_size = self.get_layout().term_area().as_size();

    loop {
      let layout = self.get_layout();

      let term_size = layout.term_area().as_size();
      if term_size != last_term_size {
        for proc_handle in &mut self.state.procs {
          self.pc.send(KernelCommand::ProcCmd(
            proc_handle.id(),
            ProcCmd::Resize {
              w: term_size.width,
              h: term_size.height,
            },
          ));
        }

        last_term_size = term_size;
      }

      if render_needed {
        if let Some((first, rest)) = self.clients.split_first_mut() {
          first.render(
            &mut self.state,
            &layout,
            &self.config,
            &self.keymap,
            &mut self.modal,
            rest,
          )?;
        }
      }

      let mut loop_action = LoopAction::default();
      if let Some(command) = self.pr.recv().await {
        self.handle_proc_command(&mut loop_action, command);
      }

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

    for client in self.clients.into_iter() {
      let mut sender = client.sender.clone();
      drop(client);
      sender.send(SrvToClt::Quit).log_ignore();
    }

    self.pc.send(KernelCommand::UnlistenProcUpdates);

    Ok(())
  }

  fn start_procs(&mut self, size: Rect) -> anyhow::Result<()> {
    let mut id_map = HashMap::with_capacity(self.config.procs.len());
    for proc_cfg in &self.config.procs {
      let proc_id = self.pc.alloc_id();
      id_map.insert(proc_cfg.name.clone(), proc_id);
    }

    let mut procs = self
      .config
      .procs
      .iter()
      .map(|proc_cfg| {
        let mut deps = Vec::new();
        for dep_name in &proc_cfg.deps {
          if let Some(dep_id) = id_map.get(dep_name) {
            deps.push(*dep_id);
          } else {
            // TODO: Show error.
          }
        }
        let proc_id = id_map.get(&proc_cfg.name).unwrap();
        launch_proc(&self.pc, proc_cfg.clone(), *proc_id, deps, size)
      })
      .collect::<Vec<_>>();

    self.state.procs.append(&mut procs);

    Ok(())
  }

  fn handle_server_message(
    &mut self,
    loop_action: &mut LoopAction,
    msg: ServerMessage,
  ) -> anyhow::Result<()> {
    match msg {
      ServerMessage::ClientMessage { client_id, msg } => {
        self.handle_client_msg(loop_action, client_id, msg)?;
      }
      ServerMessage::ClientConnected { handle } => {
        self.clients.push(handle);
        self.update_screen_size();
        loop_action.render();
      }
      ServerMessage::ClientDisconnected { client_id } => {
        self.clients.retain(|c| c.id != client_id);
        self.update_screen_size();
        loop_action.render();
      }
    }
    Ok(())
  }

  fn update_screen_size(&mut self) {
    if let Some(client) = self.clients.first_mut() {
      self.screen_size = client.size();
    }
  }

  fn handle_client_msg(
    &mut self,
    loop_action: &mut LoopAction,
    client_id: ClientId,
    msg: CltToSrv,
  ) -> anyhow::Result<()> {
    self.state.current_client_id = Some(client_id);
    let ret = match msg {
      CltToSrv::Init { .. } => bail!("Init message is unexpected."),
      CltToSrv::Key(event) => {
        self.handle_input(loop_action, client_id, event);
        Ok(())
      }
    };
    self.state.current_client_id = None;
    ret
  }

  fn handle_input(
    &mut self,
    loop_action: &mut LoopAction,
    client_id: ClientId,
    event: Event,
  ) {
    if let Event::Key(KeyEvent {
      kind: KeyEventKind::Release,
      ..
    }) = event
    {
      return;
    }

    if let Some(modal) = &mut self.modal {
      let handled = modal.handle_input(&mut self.state, loop_action, &event);
      if handled {
        return;
      }
    }

    match event {
      Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press | KeyEventKind::Repeat,
        state: _,
      }) => {
        let key = Key::new(code, modifiers);
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
      Event::Key(KeyEvent {
        kind: KeyEventKind::Release,
        ..
      }) => (),
      Event::Mouse(mev) => {
        let mouse_event = MouseEvent::from_crossterm(mev);

        let layout = self.get_layout();
        if term_check_hit(layout.term_area(), mev.column, mev.row) {
          if let (Scope::Procs, MouseEventKind::Down(_)) =
            (self.state.scope, mev.kind)
          {
            self.state.scope = Scope::Term
          }
          if let Some(proc) = self.state.get_current_proc_mut() {
            let local_event = mouse_event.translate(layout.term_area());

            proc.copy_mode = match std::mem::take(&mut proc.copy_mode) {
              CopyMode::None(pos) => {
                let mouse_mode = proc
                  .vt
                  .as_ref()
                  .map(|vt| vt.read().unwrap().screen().mouse_protocol_mode())
                  .unwrap_or_default();

                match mouse_mode {
                  MouseProtocolMode::None => match local_event.kind {
                    MouseEventKind::Down(btn) => match btn {
                      MouseButton::Left => {
                        if let Some(vt_ref) = proc.vt.as_mut() {
                          let vt = vt_ref.read().log_get().unwrap();
                          CopyMode::None(Some(
                            local_event
                              .pos_with_scrollback(vt.screen().scrollback()),
                          ))
                        } else {
                          CopyMode::None(pos)
                        }
                      }
                      MouseButton::Right | MouseButton::Middle => {
                        CopyMode::None(pos)
                      }
                    },
                    MouseEventKind::Up(_) => CopyMode::None(pos),
                    MouseEventKind::Drag(MouseButton::Left) => {
                      if let Some(vt_ref) = proc.vt.as_mut() {
                        let vt = vt_ref.read().log_get().unwrap();
                        let new_pos = local_event
                          .pos_with_scrollback(vt.screen().scrollback());
                        CopyMode::Active(
                          vt.screen().clone(),
                          pos.unwrap_or_default(),
                          Some(new_pos),
                        )
                      } else {
                        CopyMode::None(pos)
                      }
                    }
                    MouseEventKind::Drag(_) => CopyMode::None(pos),
                    MouseEventKind::Moved => CopyMode::None(pos),
                    MouseEventKind::ScrollDown => {
                      if let Some(vt_ref) = proc.vt.as_mut() {
                        let mut vt = vt_ref.write().log_get().unwrap();
                        vt.screen
                          .scroll_screen_down(self.config.mouse_scroll_speed);
                      }
                      CopyMode::None(pos)
                    }
                    MouseEventKind::ScrollUp => {
                      if let Some(vt_ref) = proc.vt.as_mut() {
                        let mut vt = vt_ref.write().log_get().unwrap();
                        vt.screen
                          .scroll_screen_up(self.config.mouse_scroll_speed);
                      }
                      CopyMode::None(pos)
                    }
                    MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight => CopyMode::None(pos),
                  },
                  MouseProtocolMode::Press
                  | MouseProtocolMode::PressRelease
                  | MouseProtocolMode::ButtonMotion
                  | MouseProtocolMode::AnyMotion => {
                    self.pc.send(KernelCommand::ProcCmd(
                      proc.id(),
                      ProcCmd::SendMouse(local_event),
                    ));
                    CopyMode::None(pos)
                  }
                }
              }
              CopyMode::Active(mut screen, start, end) => {
                match local_event.kind {
                  MouseEventKind::Down(btn) => match btn {
                    MouseButton::Left => {
                      let pos =
                        local_event.pos_with_scrollback(screen.scrollback());
                      let (start, end) = if end.is_some() {
                        (start, Some(pos))
                      } else {
                        (pos, None)
                      };
                      CopyMode::Active(screen, start, end)
                    }
                    MouseButton::Right => {
                      let pos =
                        local_event.pos_with_scrollback(screen.scrollback());
                      CopyMode::Active(screen, start, Some(pos))
                    }
                    MouseButton::Middle => CopyMode::Active(screen, start, end),
                  },
                  MouseEventKind::Up(_) => CopyMode::Active(screen, start, end),
                  MouseEventKind::Drag(MouseButton::Left) => {
                    let pos =
                      local_event.pos_with_scrollback(screen.scrollback());
                    CopyMode::Active(screen, start, Some(pos))
                  }
                  MouseEventKind::Drag(_) => {
                    CopyMode::Active(screen, start, end)
                  }
                  MouseEventKind::Moved => CopyMode::Active(screen, start, end),
                  MouseEventKind::ScrollDown => {
                    screen.scroll_screen_down(self.config.mouse_scroll_speed);
                    CopyMode::Active(screen, start, end)
                  }
                  MouseEventKind::ScrollUp => {
                    screen.scroll_screen_up(self.config.mouse_scroll_speed);
                    CopyMode::Active(screen, start, end)
                  }
                  MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                    CopyMode::Active(screen, start, end)
                  }
                }
              }
            }
          }
        } else if procs_check_hit(layout.procs, mev.column, mev.row) {
          if let (Scope::Term, MouseEventKind::Down(_)) =
            (self.state.scope, mev.kind)
          {
            self.state.scope = Scope::Procs
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
        if let Some(client) =
          self.clients.iter_mut().find(|c| c.id == client_id)
        {
          let size = Size { width, height };
          client.resize(size);
        }
        self.update_screen_size();

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
    let pc = self.pc.clone();
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
        self.modal = Some(QuitModal::new(self.pc.clone()).boxed());
        loop_action.render();
      }
      AppEvent::Quit => {
        self.state.quitting = true;
        for proc_handle in self.state.procs.iter_mut() {
          proc_handle.target_state = TargetState::Stopped;
          if proc_handle.is_up() {
            pc.send(KernelCommand::ProcCmd(proc_handle.id(), ProcCmd::Stop));
          }
        }
        loop_action.render();
      }
      AppEvent::ForceQuit => {
        for proc_handle in self.state.procs.iter_mut() {
          proc_handle.target_state = TargetState::Stopped;
          if proc_handle.is_up() {
            pc.send(KernelCommand::ProcCmd(proc_handle.id(), ProcCmd::Kill));
          }
        }
        loop_action.force_quit();
      }
      AppEvent::Detach { client_id: _ } => {
        // TODO: Client-server mode is disabled for mprocs 0.7
        // self.clients.retain_mut(|c| c.id != *client_id);
        // self.update_screen_size();
        loop_action.render();
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

      AppEvent::ShowCommandsMenu => {
        self.modal = Some(CommandsMenuModal::new(self.pc.clone()).boxed());
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
          self.state.procs.len().saturating_sub(1)
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
          proc.target_state = TargetState::Started;
          pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Start));
        }
      }
      AppEvent::TermProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Stopped;
          pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Stop));
        }
      }
      AppEvent::KillProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Stopped;
          pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Kill));
        }
      }
      AppEvent::RestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Stop));
          } else {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Start));
          }
        }
      }
      AppEvent::RestartAll => {
        for proc in &mut self.state.procs {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Stop));
          } else {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Start));
          }
        }
      }
      AppEvent::ForceRestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Kill));
          } else {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Start));
          }
        }
      }
      AppEvent::ForceRestartAll => {
        for proc in &mut self.state.procs {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Kill));
          } else {
            pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::Start));
          }
        }
      }

      AppEvent::ScrollUpLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => pc.send(KernelCommand::ProcCmd(
              proc.id,
              ProcCmd::ScrollUpLines { n: *n },
            )),
            CopyMode::Active(screen, _, _) => screen.scroll_screen_up(*n),
          }
          loop_action.render();
        }
      }
      AppEvent::ScrollDownLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => pc.send(KernelCommand::ProcCmd(
              proc.id,
              ProcCmd::ScrollDownLines { n: *n },
            )),
            CopyMode::Active(screen, _, _) => screen.scroll_screen_down(*n),
          }
          loop_action.render();
        }
      }
      AppEvent::ScrollUp => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => {
              pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::ScrollUp))
            }
            CopyMode::Active(screen, _, _) => {
              screen.scroll_screen_up(screen.size().rows as usize / 2)
            }
          }
          loop_action.render();
        }
      }
      AppEvent::ScrollDown => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => {
              pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::ScrollDown))
            }
            CopyMode::Active(screen, _, _) => {
              screen.scroll_screen_down(screen.size().rows as usize / 2)
            }
          }
          loop_action.render();
        }
      }
      AppEvent::ShowAddProc => {
        self.modal = Some(AddProcModal::new(self.pc.clone()).boxed());
        loop_action.render();
      }
      AppEvent::AddProc { cmd, name } => {
        let name = match name {
          Some(s) => s,
          None => cmd,
        };
        let proc_config = ProcConfig {
          name: name.clone(),
          cmd: CmdConfig::Shell {
            shell: cmd.to_string(),
          },
          cwd: None,
          env: None,
          autostart: true,
          autorestart: false,
          stop: StopSignal::default(),
          deps: Vec::new(),
          mouse_scroll_speed: self.config.mouse_scroll_speed,
          scrollback_len: self.config.scrollback_len,
        };
        let proc_handle = launch_proc(
          &pc,
          proc_config,
          self.pc.alloc_id(),
          Vec::new(),
          self.get_layout().term_area(),
        );
        self.state.procs.push(proc_handle);

        loop_action.render();
      }
      AppEvent::DuplicateProc => {
        let cfg = match self.state.get_current_proc_mut() {
          Some(proc_handle) => Some(proc_handle.cfg.clone()),
          None => None,
        };
        if let Some(cfg) = cfg {
          let size = self.get_layout().term_area();
          log::error!("TODO: Copy deps for duplicate proc.");
          let proc_handle =
            launch_proc(&pc, cfg, self.pc.alloc_id(), Vec::new(), size);
          self.state.procs.push(proc_handle);
          loop_action.render();
        }
      }
      AppEvent::ShowRemoveProc => {
        let id = match self.state.get_current_proc() {
          Some(proc) if !proc.is_up() => Some(proc.id()),
          _ => None,
        };
        if let Some(id) = id {
          self.modal = Some(RemoveProcModal::new(id, self.pc.clone()).boxed());
          loop_action.render();
        }
      }
      AppEvent::RemoveProc { id } => {
        self.state.procs.retain(|p| p.is_up() || p.id() != *id);
        loop_action.render();
      }

      AppEvent::CloseCurrentModal => {
        self.modal = None;
        loop_action.render();
      }

      AppEvent::ShowRenameProc => {
        self.modal = Some(RenameProcModal::new(self.pc.clone()).boxed());
        loop_action.render();
      }
      AppEvent::RenameProc { name } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.rename(name);
          loop_action.render();
        }
      }

      AppEvent::CopyModeEnter => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if let Some(vt_ref) = proc.vt.as_ref() {
            let screen = vt_ref.read().unwrap().screen().clone();
            let y = (screen.size().rows - 1) as i32;
            proc.copy_mode = CopyMode::Active(screen, Pos { y, x: 0 }, None);
          }
          self.state.scope = Scope::Term;
          loop_action.render();
        };
      }
      AppEvent::CopyModeLeave => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.copy_mode = CopyMode::None(None);
        }
        loop_action.render();
      }
      AppEvent::CopyModeMove { dir } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => (),
            CopyMode::Active(screen, start, end) => {
              let pos_ = if let Some(end) = end { end } else { start };
              match dir {
                CopyMove::Up => {
                  if pos_.y > -(screen.scrollback_len() as i32) {
                    pos_.y -= 1
                  }
                }
                CopyMove::Right => {
                  if pos_.x + 1 < screen.size().cols as i32 {
                    pos_.x += 1
                  }
                }
                CopyMove::Left => {
                  if pos_.x > 0 {
                    pos_.x -= 1
                  }
                }
                CopyMove::Down => {
                  if pos_.y + 1 < screen.size().rows as i32 {
                    pos_.y += 1
                  }
                }
              };
            }
          }
        }
        loop_action.render();
      }
      AppEvent::CopyModeEnd => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.copy_mode = match std::mem::take(&mut proc.copy_mode) {
            CopyMode::Active(screen, start, None) => {
              CopyMode::Active(screen, start.clone(), Some(start))
            }
            other => other,
          };
        }
        loop_action.render();
      }
      AppEvent::CopyModeCopy => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if let CopyMode::Active(screen, start, Some(end)) = &proc.copy_mode {
            let (low, high) = Pos::to_low_high(start, end);
            let text = screen.get_selected_text(low.x, low.y, high.x, high.y);
            crate::clipboard::copy(text.as_str());
          }
          proc.copy_mode = CopyMode::None(None);
        }
        loop_action.render();
      }

      AppEvent::ToggleKeymapWindow => {
        self.state.toggle_keymap_window();
        loop_action.render();
      }

      AppEvent::SendKey { key } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          pc.send(KernelCommand::ProcCmd(proc.id, ProcCmd::SendKey(*key)));
        }
      }
    }
  }

  fn handle_proc_command(
    &mut self,
    loop_action: &mut LoopAction,
    command: ProcCmd,
  ) {
    match command {
      ProcCmd::Start => (),
      ProcCmd::Stop => (),
      ProcCmd::Kill => (),
      ProcCmd::SendKey(_key) => (),
      ProcCmd::SendMouse(_mouse_event) => (),
      ProcCmd::ScrollUp => (),
      ProcCmd::ScrollDown => (),
      ProcCmd::ScrollUpLines { .. } => (),
      ProcCmd::ScrollDownLines { .. } => (),
      ProcCmd::Resize { .. } => (),

      ProcCmd::Custom(custom) => {
        match (custom as Box<dyn Any>).downcast::<AppEvent>() {
          Ok(app_event) => {
            self.handle_event(loop_action, &app_event);
          }
          Err(custom) => {
            match (custom as Box<dyn Any>).downcast::<ServerMessage>() {
              Ok(server_msg) => {
                let r = self.handle_server_message(loop_action, *server_msg);
                if let Err(err) = r {
                  log::debug!("ServerMessage error: {:?}", err);
                }
              }
              Err(custom) => {
                log::error!(
                  "App received unknown custom command: {:?}",
                  custom
                );
              }
            }
          }
        }
      }

      ProcCmd::OnProcUpdate(proc_id, update) => match update {
        ProcUpdate::Started => {
          if let Some(proc) = self.state.get_proc_mut(proc_id) {
            proc.is_up = true;
            proc.last_start = Some(Instant::now());
            match proc.target_state {
              TargetState::None => (),
              TargetState::Started => {
                proc.target_state = TargetState::None;
              }
              TargetState::Stopped => {
                self.pc.send(KernelCommand::ProcCmd(proc_id, ProcCmd::Stop));
              }
            }
            loop_action.render();
          }
        }
        ProcUpdate::Stopped(exit_code) => {
          if let Some(proc) = self.state.get_proc_mut(proc_id) {
            proc.is_up = false;
            proc.exit_code = Some(exit_code);

            let restart = match proc.target_state {
              TargetState::None if proc.cfg.autorestart && exit_code != 0 => {
                match proc.last_start {
                  Some(last_start) => {
                    let elapsed_time =
                      Instant::now().duration_since(last_start);
                    elapsed_time.as_secs_f64() > RESTART_THRESHOLD_SECONDS
                  }
                  None => true,
                }
              }
              TargetState::None => false,
              TargetState::Started => true,
              TargetState::Stopped => {
                proc.target_state = TargetState::None;
                false
              }
            };
            if restart {
              self
                .pc
                .send(KernelCommand::ProcCmd(proc_id, ProcCmd::Start));
            }

            match proc.target_state {
              TargetState::None => (),
              TargetState::Started => (),
              TargetState::Stopped => {
                proc.target_state = TargetState::None;
              }
            }

            if !restart {
              if self.state.all_procs_down() {
                if let Some(event) = self.config.on_all_finished.clone() {
                  self.handle_event(loop_action, &event);
                }
              }
            }

            loop_action.render();
          }
        }
        ProcUpdate::Waiting(waiting) => {
          if let Some(proc) = self.state.get_proc_mut(proc_id) {
            proc.is_waiting = waiting;
            loop_action.render();
          }
        }
        ProcUpdate::ScreenChanged(vt) => {
          if let Some(proc) = self.state.get_proc_mut(proc_id) {
            proc.vt = vt;
            proc.changed = true;
            loop_action.render();
          }
        }
        ProcUpdate::Rendered => {
          let is_current = self
            .state
            .get_current_proc()
            .is_some_and(|p| p.id() == proc_id);
          if let Some(proc) = self.state.get_proc_mut(proc_id) {
            if !is_current {
              proc.changed = true;
            }
            loop_action.render();
          }
        }
      },
    }
  }

  fn get_layout(&mut self) -> AppLayout {
    let size = self.screen_size;
    AppLayout::new(
      Rect::new(0, 0, size.width, size.height),
      self.state.scope.is_zoomed(),
      self.state.hide_keymap_window,
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
  pub fn new(
    area: Rect,
    zoom: bool,
    hide_keymap_window: bool,
    config: &Config,
  ) -> Self {
    let keymap_h = if zoom || hide_keymap_window { 0 } else { 3 };
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
    self.term.inner(Margin {
      vertical: 1,
      horizontal: 1,
    })
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientId(pub u32);

pub async fn client_loop(
  id: ClientId,
  app_sender: ProcSender,
  (client_sender, mut server_receiver): (
    MsgSender<SrvToClt>,
    MsgReceiver<CltToSrv>,
  ),
) {
  log::info!("client_loop: server_receiver.recv()");
  let init_msg = server_receiver.recv().await;
  match init_msg {
    Some(Ok(CltToSrv::Init { width, height })) => {
      let client_handle =
        ClientHandle::create(id, client_sender, Size { width, height });
      match client_handle {
        Ok(handle) => {
          app_sender
            .send(ProcCmd::custom(ServerMessage::ClientConnected { handle }));
        }
        Err(err) => {
          log::error!("Client creation error: {:?}", err);
        }
      }
    }
    _ => todo!(),
  }

  loop {
    let msg = if let Some(msg) = server_receiver.recv().await {
      msg
    } else {
      break;
    };

    match msg {
      Ok(msg) => {
        app_sender.send(ProcCmd::custom(ServerMessage::ClientMessage {
          client_id: id,
          msg,
        }));
      }
      Err(_err) => break,
    }
  }
  app_sender.send(ProcCmd::custom(ServerMessage::ClientDisconnected {
    client_id: id,
  }));
}

pub struct ClientHandle {
  id: ClientId,
  sender: MsgSender<SrvToClt>,
  terminal: Term,

  cursor_style: CursorStyle,
}

impl Debug for ClientHandle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ClientHandle")
      .field("id", &self.id)
      .finish()
  }
}

impl ClientHandle {
  fn create(
    id: ClientId,
    client_sender: MsgSender<SrvToClt>,
    size: Size,
  ) -> anyhow::Result<Self> {
    let backend = ProxyBackend {
      tx: client_sender.clone(),
      width: size.width,
      height: size.height,
      x: 0,
      y: 0,
    };
    let terminal = Terminal::new(backend)?;

    Ok(Self {
      id,
      sender: client_sender,
      terminal,

      cursor_style: CursorStyle::Default,
    })
  }

  fn size(&self) -> Size {
    let backend = self.terminal.backend();
    Size {
      width: backend.width,
      height: backend.height,
    }
  }

  fn resize(&mut self, size: Size) {
    self
      .terminal
      .backend_mut()
      .set_size(size.width, size.height);
    self
      .terminal
      .resize(Rect::new(0, 0, size.width, size.height))
      .log_ignore();
  }

  fn render(
    &mut self,
    state: &mut State,
    layout: &AppLayout,
    config: &Config,
    keymap: &Keymap,
    modal: &mut Option<Box<dyn Modal>>,
    rest: &mut [ClientHandle],
  ) -> anyhow::Result<()> {
    self.terminal.draw(|f| {
      let mut cursor_style = self.cursor_style;

      render_procs(layout.procs, f, state, config);
      render_term(layout.term, f, state, &mut cursor_style);
      render_keymap(layout.keymap, f, state, keymap);
      render_zoom_tip(layout.zoom_banner, f, keymap);

      if let Some(modal) = modal {
        cursor_style = CursorStyle::Default;
        modal.render(f);
      }

      for client_handle in rest {
        f.render_widget(RenderOtherClient(client_handle), f.area());
      }

      if self.cursor_style != cursor_style {
        self
          .sender
          .send(SrvToClt::CursorShape(cursor_style.into()))
          .log_ignore();
        self.cursor_style = cursor_style;
      }
    })?;

    Ok(())
  }

  fn render_from(&mut self, buf: &tui::buffer::Buffer) -> anyhow::Result<()> {
    self.terminal.draw(|f| {
      let area = buf.area().intersection(f.area());
      f.render_widget(CopyBuffer(buf), area);
    })?;
    Ok(())
  }
}

struct RenderOtherClient<'a>(&'a mut ClientHandle);

impl Widget for RenderOtherClient<'_> {
  fn render(self, _area: Rect, buf: &mut tui::prelude::Buffer) {
    self.0.render_from(buf).log_ignore();
  }
}

struct CopyBuffer<'a>(&'a tui::buffer::Buffer);

impl Widget for CopyBuffer<'_> {
  fn render(self, area: Rect, buf: &mut tui::prelude::Buffer) {
    for row in area.y..area.height {
      for col in area.x..area.width {
        let from = self.0.cell((col, row));
        if let Some(cell) = buf.cell_mut((col, row)) {
          *cell = from.cloned().unwrap_or_default();
        }
      }
    }
  }
}

pub fn create_app_proc(
  config: Config,
  keymap: Keymap,
  pc: &ProcContext,
) -> ProcId {
  pc.add_proc(Box::new(|pc| {
    log::debug!("Creating app proc (id: {})", pc.proc_id.0);
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async {
      let r = server_main(config, keymap, receiver, pc).await;
      match r {
        Ok(()) => (),
        Err(err) => log::error!("App proc finished with error: {:?}", err),
      }
    });
    ProcInit {
      sender,
      stop_on_quit: false,
      status: ProcStatus::Running,
      deps: Vec::new(),
    }
  }))
}

pub async fn server_main(
  config: Config,
  keymap: Keymap,
  pr: UnboundedReceiver<ProcCmd>,
  pc: ProcContext,
) -> anyhow::Result<()> {
  let state = State {
    current_client_id: None,

    scope: Scope::Procs,
    procs: Vec::new(),
    selected: 0,
    hide_keymap_window: config.hide_keymap_window,

    quitting: false,
  };

  let app = App {
    config,
    keymap,
    state,
    modal: None,
    pr,
    pc,

    screen_size: Size {
      width: 160,
      height: 50,
    },
    clients: Vec::new(),
  };
  app.run().await?;

  Ok(())
}
