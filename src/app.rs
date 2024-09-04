use anyhow::bail;
use crossterm::event::{
  Event, KeyEvent, KeyEventKind, MouseButton, MouseEventKind,
};
use futures::{future::FutureExt, select};
use serde::{Deserialize, Serialize};
use termwiz::escape::csi::CursorStyle;
use tokio::{
  io::AsyncReadExt,
  sync::mpsc::{UnboundedReceiver, UnboundedSender},
};
use tui::{
  layout::{Constraint, Direction, Layout, Margin, Rect},
  widgets::Widget,
  Terminal,
};
use vt100::Size;

use crate::{
  config::{CmdConfig, Config, ProcConfig, ServerConfig},
  error::ResultLogger,
  event::AppEvent,
  host::{
    receiver::MsgReceiver, sender::MsgSender, socket::bind_server_socket,
  },
  kernel::kernel_message::{KernelMessage, KernelSender},
  key::Key,
  keymap::Keymap,
  modal::{
    add_proc::AddProcModal, commands_menu::CommandsMenuModal, modal::Modal,
    quit::QuitModal, remove_proc::RemoveProcModal,
    rename_proc::RenameProcModal,
  },
  mouse::MouseEvent,
  proc::{
    create_proc,
    msg::{ProcCmd, ProcEvent},
    StopSignal,
  },
  protocol::{CltToSrv, ProxyBackend, SrvToClt},
  state::{Scope, State},
  ui_keymap::render_keymap,
  ui_procs::{procs_check_hit, procs_get_clicked_index, render_procs},
  ui_term::{render_term, term_check_hit},
  ui_zoom_tip::render_zoom_tip,
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
  proc_rx: UnboundedReceiver<(usize, ProcEvent)>,
  proc_tx: UnboundedSender<(usize, ProcEvent)>,
  ev_rx: UnboundedReceiver<AppEvent>,
  ev_tx: UnboundedSender<AppEvent>,
  // kernel_sender: KernelSender,
  kernel_receiver: tokio::sync::mpsc::UnboundedReceiver<KernelMessage>,

  screen_size: Size,
  clients: Vec<ClientHandle>,
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
    self.start_procs(Rect::new(
      0,
      0,
      self.screen_size.width,
      self.screen_size.height,
    ))?;

    let mut render_needed = true;
    loop {
      if render_needed {
        let layout = self.get_layout();

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
      let () = select! {
        event = self.kernel_receiver.recv().fuse() => {
          if let Some(event) = event {
            self.handle_kernel_message(&mut loop_action, event)?
          }
        }
        event = self.proc_rx.recv().fuse() => {
          if let Some(event) = event {
            self.handle_proc_event(&mut loop_action, event)
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

    for client in self.clients.into_iter() {
      let mut sender = client.sender.clone();
      drop(client);
      sender.send(SrvToClt::Quit).log_ignore();
    }

    Ok(())
  }

  fn start_procs(&mut self, size: Rect) -> anyhow::Result<()> {
    let mut procs = self
      .config
      .procs
      .iter()
      .map(|proc_cfg| {
        create_proc(proc_cfg.name.clone(), proc_cfg, self.proc_tx.clone(), size)
      })
      .collect::<Vec<_>>();

    self.state.procs.append(&mut procs);

    Ok(())
  }

  fn handle_kernel_message(
    &mut self,
    loop_action: &mut LoopAction,
    msg: KernelMessage,
  ) -> anyhow::Result<()> {
    match msg {
      KernelMessage::ClientMessage { client_id, msg } => {
        self.handle_client_msg(loop_action, client_id, msg)?;
      }
      KernelMessage::ClientConnected { handle } => {
        self.clients.push(handle);
        self.update_screen_size();
        loop_action.render();
      }
      KernelMessage::ClientDisconnected { client_id } => {
        self.clients.retain(|c| c.id != client_id);
        self.update_screen_size();
        loop_action.render();
      }
    }
    Ok(())
  }

  fn update_screen_size(&mut self) {
    if let Some(client) = self.clients.first_mut() {
      let size = client.size();
      if self.screen_size != size {
        self.screen_size = size;
        self.sync_proc_handle_size();
      }
    }
  }

  fn sync_proc_handle_size(&mut self) {
    let area = self.get_layout().term_area();
    for proc_handle in &mut self.state.procs {
      proc_handle.send(ProcCmd::Resize {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
      });
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
    match event {
      Event::Key(KeyEvent {
        kind: KeyEventKind::Release,
        ..
      }) => return,
      _ => (),
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
        if mev.kind == MouseEventKind::Moved {
          return;
        }

        let mouse_event = MouseEvent::from_crossterm(mev);

        let layout = self.get_layout();
        if term_check_hit(layout.term_area(), mev.column, mev.row) {
          match (self.state.scope, mev.kind) {
            (Scope::Procs, MouseEventKind::Down(_)) => {
              self.state.scope = Scope::Term
            }
            _ => (),
          }
          if let Some(proc) = self.state.get_current_proc_mut() {
            proc.send(ProcCmd::SendMouse(
              mouse_event.translate(layout.term_area()),
            ));
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
        self.modal = Some(QuitModal::new(self.ev_tx.clone()).boxed());
        loop_action.render();
      }
      AppEvent::Quit => {
        self.state.quitting = true;
        for proc_handle in self.state.procs.iter_mut() {
          if proc_handle.is_up() {
            proc_handle.send(ProcCmd::Stop);
          }
        }
        loop_action.render();
      }
      AppEvent::ForceQuit => {
        for proc_handle in self.state.procs.iter_mut() {
          if proc_handle.is_up() {
            proc_handle.send(ProcCmd::Kill);
          }
        }
        loop_action.force_quit();
      }
      AppEvent::Detach { client_id } => {
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
        self.modal = Some(CommandsMenuModal::new(self.ev_tx.clone()).boxed());
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
          proc.send(ProcCmd::Start);
        }
      }
      AppEvent::TermProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::Stop);
        }
      }
      AppEvent::KillProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::Kill);
        }
      }
      AppEvent::RestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if proc.is_up() {
            proc.to_restart = true;
            proc.send(ProcCmd::Stop);
          } else {
            proc.send(ProcCmd::Start);
          }
        }
      }
      AppEvent::ForceRestartProc => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          if proc.is_up() {
            proc.to_restart = true;
            proc.send(ProcCmd::Kill);
          } else {
            proc.send(ProcCmd::Start);
          }
        }
      }

      AppEvent::ScrollUpLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::ScrollUpLines { n: *n });
          loop_action.render();
        }
      }
      AppEvent::ScrollDownLines { n } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::ScrollDownLines { n: *n });
          loop_action.render();
        }
      }
      AppEvent::ScrollUp => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::ScrollUp);
          loop_action.render();
        }
      }
      AppEvent::ScrollDown => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::ScrollDown);
          loop_action.render();
        }
      }
      AppEvent::ShowAddProc => {
        self.modal = Some(AddProcModal::new(self.ev_tx.clone()).boxed());
        loop_action.render();
      }
      AppEvent::AddProc { cmd, name } => {
        let name: String = match name {
          Some(s) => s.to_string(),
          None => cmd.to_string(),
        };
        let proc_handle = create_proc(
          name.clone(),
          &ProcConfig {
            name: name.clone(),
            cmd: CmdConfig::Shell {
              shell: cmd.to_string(),
            },
            cwd: None,
            env: None,
            autostart: true,
            autorestart: false,
            stop: StopSignal::default(),
            mouse_scroll_speed: self.config.mouse_scroll_speed,
            scrollback_len: self.config.scrollback_len,
          },
          self.proc_tx.clone(),
          self.get_layout().term_area(),
        );
        self.state.procs.push(proc_handle);
        loop_action.render();
      }
      AppEvent::DuplicateProc => {
        if let Some(proc_handle) = self.state.get_current_proc_mut() {
          let proc_handle = proc_handle.duplicate();
          self.state.procs.push(proc_handle);
        }
        loop_action.render();
      }
      AppEvent::ShowRemoveProc => {
        let id = self
          .state
          .get_current_proc()
          .map(|proc| if proc.is_up() { None } else { Some(proc.id()) })
          .flatten();
        match id {
          Some(id) => {
            self.modal =
              Some(RemoveProcModal::new(id, self.ev_tx.clone()).boxed());
            loop_action.render();
          }
          None => (),
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
        self.modal = Some(RenameProcModal::new(self.ev_tx.clone()).boxed());
        loop_action.render();
      }
      AppEvent::RenameProc { name } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.rename(name);
          loop_action.render();
        }
      }

      AppEvent::CopyModeEnter => {
        match self.state.get_current_proc_mut() {
          Some(proc) => {
            proc.send(ProcCmd::CopyModeEnter);
            self.state.scope = Scope::Term;
            loop_action.render();
          }
          None => (),
        };
      }
      AppEvent::CopyModeLeave => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::CopyModeLeave);
        }
        loop_action.render();
      }
      AppEvent::CopyModeMove { dir } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::CopyModeMove { dir: *dir });
        }
        loop_action.render();
      }
      AppEvent::CopyModeEnd => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::CopyModeEnd);
        }
        loop_action.render();
      }
      AppEvent::CopyModeCopy => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::CopyModeCopy);
        }
        loop_action.render();
      }

      AppEvent::ToggleKeymapWindow => {
        self.state.toggle_keymap_window();
        self.sync_proc_handle_size();
        loop_action.render();
      }

      AppEvent::SendKey { key } => {
        if let Some(proc) = self.state.get_current_proc_mut() {
          proc.send(ProcCmd::SendKey(key.clone()));
        }
      }
    }
  }

  fn handle_proc_event(
    &mut self,
    loop_action: &mut LoopAction,
    event: (usize, ProcEvent),
  ) {
    let selected = self
      .state
      .get_current_proc()
      .map_or(false, |p| p.id() == event.0);
    if let Some(proc) = self.state.get_proc_mut(event.0) {
      proc.handle_event(event.1, selected);
      loop_action.render();
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
    self.term.inner(&Margin {
      vertical: 1,
      horizontal: 1,
    })
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientId(u32);

struct ClientConnector;

impl ClientConnector {
  fn connect(
    id: ClientId,
    (sender, mut receiver): (MsgSender<SrvToClt>, MsgReceiver<CltToSrv>),
    kernel_sender: KernelSender,
  ) -> Self {
    tokio::spawn(async move {
      let init_msg = receiver.recv().await;
      match init_msg {
        Some(Ok(CltToSrv::Init { width, height })) => {
          let client_handle = ClientHandle::create(
            id,
            (receiver, sender),
            kernel_sender.clone(),
            Size { width, height },
          );
          match client_handle {
            Ok(handle) => {
              kernel_sender
                .send(KernelMessage::ClientConnected { handle })
                .log_ignore();
            }
            Err(err) => {
              log::error!("Client creation error: {:?}", err);
            }
          }
        }
        _ => todo!(),
      }
    });

    ClientConnector
  }
}

pub struct ClientHandle {
  id: ClientId,
  sender: MsgSender<SrvToClt>,
  terminal: Term,

  cursor_style: CursorStyle,
}

impl ClientHandle {
  fn create(
    id: ClientId,
    (mut read, write): (MsgReceiver<CltToSrv>, MsgSender<SrvToClt>),
    kernel_sender: KernelSender,
    size: Size,
  ) -> anyhow::Result<Self> {
    {
      let kernel_sender = kernel_sender.clone();
      tokio::spawn(async move {
        loop {
          let msg = if let Some(msg) = read.recv().await {
            msg
          } else {
            break;
          };

          match msg {
            Ok(msg) => {
              kernel_sender
                .send(
                  crate::kernel::kernel_message::KernelMessage::ClientMessage {
                    client_id: id,
                    msg,
                  },
                )
                .log_ignore();
            }
            Err(_err) => break,
          }
        }
        kernel_sender
          .send(KernelMessage::ClientDisconnected { client_id: id })
          .log_ignore();
      });
    }

    let backend = ProxyBackend {
      tx: write.clone(),
      width: size.width,
      height: size.height,
      x: 0,
      y: 0,
    };
    let terminal = Terminal::new(backend)?;

    Ok(Self {
      id,
      sender: write,
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
    _config: &Config,
    keymap: &Keymap,
    modal: &mut Option<Box<dyn Modal>>,
    rest: &mut [ClientHandle],
  ) -> anyhow::Result<()> {
    self.terminal.draw(|f| {
      let mut cursor_style = self.cursor_style;

      render_procs(layout.procs, f, state);
      render_term(layout.term, f, state, &mut cursor_style);
      render_keymap(layout.keymap, f, state, keymap);
      render_zoom_tip(layout.zoom_banner, f, keymap);

      if let Some(modal) = modal {
        cursor_style = CursorStyle::Default;
        modal.render(f);
      }

      for client_handle in rest {
        f.render_widget(RenderOtherClient(client_handle), f.size());
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
      let area = buf.area().intersection(f.size());
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
        let from = self.0.get(col, row);
        *buf.get_mut(col, row) = from.clone();
      }
    }
  }
}

pub async fn start_kernel_process(
  config: Config,
  keymap: Keymap,
) -> anyhow::Result<()> {
  let (kernel_sender, kernel_receiver) = tokio::sync::mpsc::unbounded_channel();

  let mut server_socket = bind_server_socket().await?;
  let _accept_thread = {
    let kernel_sender = kernel_sender.clone();
    tokio::spawn(async move {
      let mut last_client_id = 0;

      log::debug!("Waiting for clients...");
      loop {
        match server_socket.accept().await {
          Ok(socket) => {
            last_client_id += 1;
            let id = ClientId(last_client_id);
            ClientConnector::connect(id, socket, kernel_sender.clone());
          }
          Err(err) => {
            log::info!("Server socket accept error: {}", err.to_string());
            break;
          }
        }
      }
    })
  };

  kernel_main(config, keymap, kernel_receiver).await
}

pub async fn start_kernel_thread(
  config: Config,
  keymap: Keymap,
  socket: (MsgSender<SrvToClt>, MsgReceiver<CltToSrv>),
) -> anyhow::Result<()> {
  let (kernel_sender, kernel_receiver) = tokio::sync::mpsc::unbounded_channel();

  let id = ClientId(1);
  ClientConnector::connect(id, socket, kernel_sender.clone());

  tokio::spawn(async {
    kernel_main(config, keymap, kernel_receiver).await;
  });

  Ok(())
}

pub async fn kernel_main(
  config: Config,
  keymap: Keymap,
  kernel_receiver: UnboundedReceiver<KernelMessage>,
) -> anyhow::Result<()> {
  let (upd_tx, upd_rx) =
    tokio::sync::mpsc::unbounded_channel::<(usize, ProcEvent)>();
  let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();

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
    proc_rx: upd_rx,
    proc_tx: upd_tx,

    ev_rx,
    ev_tx,

    kernel_receiver,

    screen_size: Size {
      width: 160,
      height: 50,
    },
    clients: Vec::new(),
  };
  app.run().await?;

  Ok(())
}
