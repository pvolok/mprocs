use std::{collections::HashMap, fmt::Debug, time::Instant};

use anyhow::bail;
use futures::{future::FutureExt, select};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, sync::mpsc::UnboundedReceiver};

use crate::{
  config::{CmdConfig, Config, ProcConfig, ServerConfig},
  daemon::{receiver::MsgReceiver, sender::MsgSender},
  error::ResultLogger,
  event::{AppEvent, CopyMove},
  kernel::{
    kernel_message::{KernelCommand, TaskContext, TaskSender},
    task::{ChannelTask, TaskCmd, TaskId, TaskInit, TaskNotify, TaskStatus},
  },
  keymap::Keymap,
  modal::{
    add_proc::AddProcModal, commands_menu::CommandsMenuModal, modal::Modal,
    quit::QuitModal, remove_proc::RemoveProcModal,
    rename_proc::RenameProcModal,
  },
  proc::{
    msg::ProcMsg,
    proc::launch_proc,
    view::{TargetState, RESTART_THRESHOLD_SECONDS},
    CopyMode, Pos, StopSignal,
  },
  protocol::{CltToSrv, SrvToClt},
  server::server_message::ServerMessage,
  state::{Scope, State},
  term::{
    attrs::Attrs,
    grid::Rect,
    key::{Key, KeyEventKind},
    mouse::{MouseButton, MouseEventKind},
    Grid, MouseProtocolMode, ScreenDiffer, Size, TermEvent,
  },
  ui_keymap::render_keymap,
  ui_procs::{procs_check_hit, procs_get_clicked_index, render_procs},
  ui_term::{render_term, term_check_hit},
  ui_zoom_tip::render_zoom_tip,
  widgets::list::ListState,
};

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
  grid: Grid,
  modal: Option<Box<dyn Modal>>,
  pr: tokio::sync::mpsc::UnboundedReceiver<TaskCmd>,
  pc: TaskContext,

  screen_size: Size,
  clients: Vec<ClientHandle>,
}

impl App {
  pub async fn run(self) -> anyhow::Result<()> {
    let (exit_trigger, exit_listener) = triggered::trigger();

    let app_task_id = self.pc.task_id;
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
            ctl_pc.send(KernelCommand::TaskCmd(app_task_id, TaskCmd::msg(msg)));
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
    self.pc.send(KernelCommand::ListenTaskUpdates);

    self.start_procs(Rect {
      x: 0,
      y: 0,
      width: self.screen_size.width,
      height: self.screen_size.height,
    })?;

    let mut render_needed = true;
    let mut last_term_size = self.get_layout().term_area().size();

    let mut command_buf = Vec::new();

    loop {
      let layout = self.get_layout();

      let term_size = layout.term_area().size();
      if term_size != last_term_size {
        for proc_handle in &mut self.state.procs {
          self.pc.send(KernelCommand::TaskCmd(
            proc_handle.id(),
            TaskCmd::msg(ProcMsg::Resize {
              w: term_size.width,
              h: term_size.height,
            }),
          ));
        }

        last_term_size = term_size;
      }

      if render_needed && self.clients.len() > 0 {
        let grid = &mut self.grid;
        grid.erase_all(Attrs::default());
        grid.cursor_pos = None;
        grid.cursor_style = crate::term::CursorStyle::Default;

        let state = &mut self.state;
        let config = &mut self.config;
        let keymap = &self.keymap;
        render_procs(layout.procs.into(), grid, state, config);
        render_term(layout.term, grid, state);
        render_keymap(layout.keymap.into(), grid, state, keymap);
        render_zoom_tip(layout.zoom_banner.into(), grid, keymap);

        if let Some(modal) = &mut self.modal {
          grid.cursor_style = crate::term::CursorStyle::Default;
          modal.render(grid);
        }

        for client_handle in &mut self.clients {
          let mut out = String::new();
          client_handle.differ.diff(&mut out, grid).log_ignore();
          client_handle
            .sender
            .send(SrvToClt::Print(out))
            .await
            .unwrap();
          client_handle.sender.send(SrvToClt::Flush).await.unwrap();
        }
      }

      let mut loop_action = LoopAction::default();
      self.pr.recv_many(&mut command_buf, 512).await;
      for command in command_buf.drain(..) {
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

    for mut client in self.clients.into_iter() {
      client.sender.send(SrvToClt::Quit).await.log_ignore();
    }

    self.pc.send(KernelCommand::UnlistenTaskUpdates);

    Ok(())
  }

  fn start_procs(&mut self, size: Rect) -> anyhow::Result<()> {
    let mut id_map = HashMap::with_capacity(self.config.procs.len());
    for proc_cfg in &self.config.procs {
      let task_id = self.pc.alloc_id();
      id_map.insert(proc_cfg.name.clone(), task_id);
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
        let task_id = id_map.get(&proc_cfg.name).unwrap();
        launch_proc(&self.pc, proc_cfg.clone(), *task_id, deps, size)
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
      self.grid.set_size(client.size());
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
    event: TermEvent,
  ) {
    if let TermEvent::Key(Key {
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
      TermEvent::Key(Key {
        code,
        mods,
        kind: KeyEventKind::Press | KeyEventKind::Repeat,
        state: _,
      }) => {
        let key = Key::new(code, mods);
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
      TermEvent::Key(Key {
        kind: KeyEventKind::Release,
        ..
      }) => (),
      TermEvent::Mouse(mouse_event) => {
        let layout = self.get_layout();
        if term_check_hit(
          layout.term_area(),
          mouse_event.x as u16,
          mouse_event.y as u16,
        ) {
          if let (Scope::Procs, MouseEventKind::Down(_)) =
            (self.state.scope, mouse_event.kind)
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
                          .scroll_screen_down(proc.cfg.mouse_scroll_speed);
                      }
                      CopyMode::None(pos)
                    }
                    MouseEventKind::ScrollUp => {
                      if let Some(vt_ref) = proc.vt.as_mut() {
                        let mut vt = vt_ref.write().log_get().unwrap();
                        vt.screen.scroll_screen_up(proc.cfg.mouse_scroll_speed);
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
                    self.pc.send(KernelCommand::TaskCmd(
                      proc.id(),
                      TaskCmd::msg(ProcMsg::SendMouse(local_event)),
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
        } else if procs_check_hit(
          layout.procs.into(),
          mouse_event.x as u16,
          mouse_event.y as u16,
        ) {
          if let (Scope::Term, MouseEventKind::Down(_)) =
            (self.state.scope, mouse_event.kind)
          {
            self.state.scope = Scope::Procs
          }
          match mouse_event.kind {
            MouseEventKind::Down(btn) => match btn {
              MouseButton::Left => {
                if let Some(index) = procs_get_clicked_index(
                  layout.procs.into(),
                  mouse_event.x as u16,
                  mouse_event.y as u16,
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
              if self.state.selected()
                < self.state.procs.len().saturating_sub(1)
              {
                let index = self.state.selected() + 1;
                self.state.select_proc(index);
              }
            }
            MouseEventKind::ScrollUp => {
              if self.state.selected() > 0 {
                let index = self.state.selected() - 1;
                self.state.select_proc(index);
              }
            }
            MouseEventKind::ScrollLeft => (),
            MouseEventKind::ScrollRight => (),
          }
        }
        loop_action.render();
      }
      TermEvent::Resize(width, height) => {
        if let Some(client) =
          self.clients.iter_mut().find(|c| c.id == client_id)
        {
          let size = Size { width, height };
          client.resize(size);
        }
        self.update_screen_size();

        loop_action.render();
      }
      TermEvent::FocusGained => {
        log::warn!("Ignore input event: {:?}", event);
      }
      TermEvent::FocusLost => {
        log::warn!("Ignore input event: {:?}", event);
      }
      TermEvent::Paste(_) => {
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
            pc.send(KernelCommand::TaskCmd(proc_handle.id(), TaskCmd::Stop));
          }
        }
        loop_action.render();
      }
      AppEvent::ForceQuit => {
        for proc_handle in self.state.procs.iter_mut() {
          proc_handle.target_state = TargetState::Stopped;
          if proc_handle.is_up() {
            pc.send(KernelCommand::TaskCmd(proc_handle.id(), TaskCmd::Kill));
          }
        }
        loop_action.force_quit();
      }
      AppEvent::Detach { client_id } => {
        self.clients.retain_mut(|c| c.id != *client_id);
        self.update_screen_size();
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
        self.modal =
          Some(CommandsMenuModal::new(self.pc.clone(), &self.keymap).boxed());
        loop_action.render();
      }
      AppEvent::NextProc => {
        let mut next = self.state.selected() + 1;
        if next >= self.state.procs.len() {
          next = 0;
        }
        self.state.select_proc(next);
        loop_action.render();
      }
      AppEvent::PrevProc => {
        let next = if self.state.selected() > 0 {
          self.state.selected() - 1
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
          pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Start));
        }
      }
      AppEvent::TermProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Stopped;
          pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Stop));
        }
      }
      AppEvent::KillProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Stopped;
          pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Kill));
        }
      }
      AppEvent::RestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Stop));
          } else {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Start));
          }
        }
      }
      AppEvent::RestartAll => {
        for proc in &mut self.state.procs {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Stop));
          } else {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Start));
          }
        }
      }
      AppEvent::ForceRestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Kill));
          } else {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Start));
          }
        }
      }
      AppEvent::ForceRestartAll => {
        for proc in &mut self.state.procs {
          proc.target_state = TargetState::Started;
          if proc.is_up() {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Kill));
          } else {
            pc.send(KernelCommand::TaskCmd(proc.id, TaskCmd::Start));
          }
        }
      }

      AppEvent::ScrollUpLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => pc.send(KernelCommand::TaskCmd(
              proc.id,
              TaskCmd::msg(ProcMsg::ScrollUpLines { n: *n }),
            )),
            CopyMode::Active(screen, _, _) => screen.scroll_screen_up(*n),
          }
          loop_action.render();
        }
      }
      AppEvent::ScrollDownLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => pc.send(KernelCommand::TaskCmd(
              proc.id,
              TaskCmd::msg(ProcMsg::ScrollDownLines { n: *n }),
            )),
            CopyMode::Active(screen, _, _) => screen.scroll_screen_down(*n),
          }
          loop_action.render();
        }
      }
      AppEvent::ScrollUp => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => pc.send(KernelCommand::TaskCmd(
              proc.id,
              TaskCmd::msg(ProcMsg::ScrollUp),
            )),
            CopyMode::Active(screen, _, _) => {
              screen.scroll_screen_up(screen.size().height as usize / 2)
            }
          }
          loop_action.render();
        }
      }
      AppEvent::ScrollDown => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          match &mut proc.copy_mode {
            CopyMode::None(_) => pc.send(KernelCommand::TaskCmd(
              proc.id,
              TaskCmd::msg(ProcMsg::ScrollDown),
            )),
            CopyMode::Active(screen, _, _) => {
              screen.scroll_screen_down(screen.size().height as usize / 2)
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
          log: self.config.proc_log.clone(),
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
            let y = (screen.size().height - 1) as i32;
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
                  if pos_.x + 1 < screen.size().width as i32 {
                    pos_.x += 1
                  }
                }
                CopyMove::Left => {
                  if pos_.x > 0 {
                    pos_.x -= 1
                  }
                }
                CopyMove::Down => {
                  if pos_.y + 1 < screen.size().height as i32 {
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
          pc.send(KernelCommand::TaskCmd(
            proc.id,
            TaskCmd::msg(ProcMsg::SendKey(*key)),
          ));
        }
      }
    }
  }

  fn handle_proc_command(
    &mut self,
    loop_action: &mut LoopAction,
    command: TaskCmd,
  ) {
    match command {
      TaskCmd::Start | TaskCmd::Stop | TaskCmd::Kill => (),

      TaskCmd::Msg(msg) => match msg.downcast::<AppEvent>() {
        Ok(app_event) => {
          self.handle_event(loop_action, &app_event);
        }
        Err(msg) => match msg.downcast::<ServerMessage>() {
          Ok(server_msg) => {
            let r = self.handle_server_message(loop_action, *server_msg);
            if let Err(err) = r {
              log::debug!("ServerMessage error: {:?}", err);
            }
          }
          Err(_msg) => {
            log::error!("App received unknown Msg");
          }
        },
      },

      TaskCmd::Notify(task_id, notify) => match notify {
        TaskNotify::Started => {
          if let Some(proc) = self.state.get_proc_mut(task_id) {
            proc.is_up = true;
            proc.last_start = Some(Instant::now());
            match proc.target_state {
              TargetState::None => (),
              TargetState::Started => {
                proc.target_state = TargetState::None;
              }
              TargetState::Stopped => {
                self.pc.send(KernelCommand::TaskCmd(task_id, TaskCmd::Stop));
              }
            }
            loop_action.render();
          }
        }
        TaskNotify::Stopped(exit_code) => {
          if let Some(proc) = self.state.get_proc_mut(task_id) {
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
                .send(KernelCommand::TaskCmd(task_id, TaskCmd::Start));
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
        TaskNotify::ScreenChanged(vt) => {
          if let Some(proc) = self.state.get_proc_mut(task_id) {
            proc.vt = vt;
            proc.changed = true;
            loop_action.render();
          }
        }
        TaskNotify::Rendered => {
          let is_current = self
            .state
            .get_current_proc()
            .is_some_and(|p| p.id() == task_id);
          if let Some(proc) = self.state.get_proc_mut(task_id) {
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
    let (top, keymap) = area.split_h(area.height.saturating_sub(keymap_h));
    let (procs, term) = top.split_v(procs_w);
    let (zoom_banner, term) = term.split_h(zoom_banner_h);

    Self {
      procs,
      term,
      keymap,
      zoom_banner,
    }
  }

  pub fn term_area(&self) -> Rect {
    self.term.inner(1)
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientId(pub u32);

pub async fn client_loop(
  id: ClientId,
  app_sender: TaskSender,
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
            .send(TaskCmd::msg(ServerMessage::ClientConnected { handle }));
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
        app_sender.send(TaskCmd::msg(ServerMessage::ClientMessage {
          client_id: id,
          msg,
        }));
      }
      Err(_err) => break,
    }
  }
  app_sender.send(TaskCmd::msg(ServerMessage::ClientDisconnected {
    client_id: id,
  }));
}

pub struct ClientHandle {
  id: ClientId,
  sender: MsgSender<SrvToClt>,
  screen_size: Size,
  differ: ScreenDiffer,
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
    Ok(Self {
      id,
      sender: client_sender,
      screen_size: size,
      differ: ScreenDiffer::new(),
    })
  }

  fn size(&self) -> Size {
    self.screen_size
  }

  fn resize(&mut self, size: Size) {
    self.screen_size = size;
  }
}

pub fn create_app_task(
  config: Config,
  keymap: Keymap,
  pc: &TaskContext,
) -> TaskId {
  pc.add_task(Box::new(|pc| {
    log::debug!("Creating app task (id: {})", pc.task_id.0);
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async {
      let pc = pc;
      let r = server_main(config, keymap, receiver, pc.clone()).await;
      match r {
        Ok(()) => (),
        Err(err) => log::error!("App task finished with error: {:?}", err),
      };
      pc.send(KernelCommand::Quit);
    });
    TaskInit {
      task: Box::new(ChannelTask::new(sender)),
      stop_on_quit: false,
      status: TaskStatus::Running,
      deps: Vec::new(),
    }
  }))
}

pub async fn server_main(
  config: Config,
  keymap: Keymap,
  pr: UnboundedReceiver<TaskCmd>,
  pc: TaskContext,
) -> anyhow::Result<()> {
  let state = State {
    current_client_id: None,

    scope: Scope::Procs,
    procs: Vec::new(),
    procs_list: ListState::default(),
    hide_keymap_window: config.hide_keymap_window,

    quitting: false,
  };

  let size = Size {
    width: 160,
    height: 50,
  };
  let scrollback_len = config.scrollback_len;

  let app = App {
    config,
    keymap,
    state,
    grid: Grid::new(size, scrollback_len),
    modal: None,
    pr,
    pc,

    screen_size: size,
    clients: Vec::new(),
  };

  if let Some(event) = app.config.on_init.clone() {
    app.pc.send_self_custom(event);
  }

  app.run().await?;

  Ok(())
}
