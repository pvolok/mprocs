use tokio::sync::mpsc::UnboundedReceiver;

use crate::mprocs::app::{ClientHandle, ClientId};
use crate::{
  error::ResultLogger,
  kernel::kernel_message::{
    KernelCommand, KernelQuery, KernelQueryResponse, TaskContext,
  },
  kernel::task::{
    ChannelTask, TaskCmd, TaskId, TaskInit, TaskNotify, TaskStatus,
  },
  protocol::{CltToSrv, SrvToClt},
  server::server_message::ServerMessage,
  term::{
    Color, Grid, Size, TermEvent,
    attrs::Attrs,
    grid::Rect,
    key::{KeyCode, KeyEventKind, KeyMods},
  },
};

struct DkTaskEntry {
  path: String,
  status: TaskStatus,
}

struct DkApp {
  pc: TaskContext,
  pr: UnboundedReceiver<TaskCmd>,
  clients: Vec<ClientHandle>,
  grid: Grid,
  screen_size: Size,

  tasks: Vec<DkTaskEntry>,
  selected: usize,
}

impl DkApp {
  async fn run(mut self) {
    self.pc.send(KernelCommand::ListenTaskUpdates);
    self.refresh_tasks().await;

    let mut render_needed = true;
    let mut command_buf = Vec::new();

    loop {
      if render_needed && !self.clients.is_empty() {
        self.render().await;
        render_needed = false;
      }

      if self.pr.recv_many(&mut command_buf, 512).await == 0 {
        break;
      }
      for cmd in command_buf.drain(..) {
        if self.handle_cmd(cmd).await {
          render_needed = true;
        }
      }
    }
  }

  async fn handle_cmd(&mut self, cmd: TaskCmd) -> bool {
    match cmd {
      TaskCmd::Msg(msg) => {
        if let Ok(server_msg) = msg.downcast::<ServerMessage>() {
          return self.handle_server_msg(*server_msg).await;
        }
        false
      }
      TaskCmd::Notify(_task_id, notify) => match notify {
        TaskNotify::Started | TaskNotify::Stopped(_) => {
          self.refresh_tasks().await;
          true
        }
        _ => false,
      },
      _ => false,
    }
  }

  async fn handle_server_msg(&mut self, msg: ServerMessage) -> bool {
    match msg {
      ServerMessage::ClientConnected { handle } => {
        self.clients.push(handle);
        self.update_screen_size();
        true
      }
      ServerMessage::ClientDisconnected { client_id } => {
        self.clients.retain(|c| c.id != client_id);
        self.update_screen_size();
        true
      }
      ServerMessage::ClientMessage { client_id, msg } => match msg {
        CltToSrv::Key(event) => self.handle_key(client_id, event).await,
        CltToSrv::Init { width, height } => {
          self.screen_size = Size { width, height };
          self.grid.set_size(self.screen_size);
          true
        }
        CltToSrv::Rpc(_) => false,
      },
    }
  }

  async fn handle_key(
    &mut self,
    client_id: ClientId,
    event: TermEvent,
  ) -> bool {
    let key = match event {
      TermEvent::Key(k) if k.kind != KeyEventKind::Release => k,
      _ => return false,
    };

    match key.code {
      KeyCode::Char('j') | KeyCode::Down if key.mods == KeyMods::NONE => {
        self.move_selection(1);
        true
      }
      KeyCode::Char('k') | KeyCode::Up if key.mods == KeyMods::NONE => {
        self.move_selection(-1);
        true
      }
      KeyCode::Char('q') if key.mods == KeyMods::NONE => {
        if let Some(client) =
          self.clients.iter_mut().find(|c| c.id == client_id)
        {
          let _ = client.sender.send(SrvToClt::Quit).await;
        }
        true
      }
      _ => false,
    }
  }

  fn move_selection(&mut self, delta: i32) {
    if self.tasks.is_empty() {
      return;
    }
    let len = self.tasks.len() as i32;
    let new = (self.selected as i32 + delta).rem_euclid(len);
    self.selected = new as usize;
  }

  async fn refresh_tasks(&mut self) {
    let rx = self.pc.query(KernelQuery::ListTasks(None));
    if let Ok(KernelQueryResponse::TaskList(list)) = rx.await {
      self.tasks = list
        .into_iter()
        .map(|t| DkTaskEntry {
          path: t
            .path
            .map(|p| p.to_string())
            .unwrap_or_else(|| format!("<task:{}>", t.id.0)),
          status: t.status,
        })
        .collect();
      if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
        self.selected = self.tasks.len() - 1;
      }
    }
  }

  fn update_screen_size(&mut self) {
    if let Some(client) = self.clients.first() {
      self.screen_size = client.size();
      self.grid.set_size(self.screen_size);
    }
  }

  async fn render(&mut self) {
    let grid = &mut self.grid;
    grid.erase_all(Attrs::default());
    grid.cursor_pos = None;

    let w = self.screen_size.width;
    let h = self.screen_size.height;

    if w < 4 || h < 3 {
      return;
    }

    // Title bar
    let title = " dekit ";
    let title_attrs = Attrs::default()
      .fg(Color::BLACK)
      .bg(Color::WHITE)
      .set_bold(true);
    let bar_attrs = Attrs::default().fg(Color::BLACK).bg(Color::WHITE);
    for x in 0..w {
      grid.draw_text(Rect::new(x, 0, 1, 1), " ", bar_attrs);
    }
    grid.draw_text(Rect::new(1, 0, w.saturating_sub(2), 1), title, title_attrs);

    // Task list
    if self.tasks.is_empty() {
      let msg = "No tasks";
      let attrs = Attrs::default().fg(Color::Idx(245));
      grid.draw_text(Rect::new(2, 2, w.saturating_sub(4), 1), msg, attrs);
    } else {
      let max_rows = (h.saturating_sub(2)) as usize;
      let start = if self.selected >= max_rows {
        self.selected - max_rows + 1
      } else {
        0
      };

      for (i, task) in self.tasks.iter().enumerate().skip(start).take(max_rows)
      {
        let row = (i - start + 1) as u16;
        let is_selected = i == self.selected;

        let line_attrs = if is_selected {
          Attrs::default().bg(Color::Idx(236))
        } else {
          Attrs::default()
        };

        if is_selected {
          for x in 0..w {
            grid.draw_text(Rect::new(x, row, 1, 1), " ", line_attrs);
          }
        }

        // Status indicator
        let (status_char, status_color) = match task.status {
          TaskStatus::Running => ("●", Color::GREEN),
          TaskStatus::Down => ("○", Color::RED),
        };
        let status_attrs = if is_selected {
          Attrs::default().fg(status_color).bg(Color::Idx(236))
        } else {
          Attrs::default().fg(status_color)
        };
        grid.draw_text(Rect::new(1, row, 2, 1), status_char, status_attrs);

        // Task path
        grid.draw_text(
          Rect::new(3, row, w.saturating_sub(4), 1),
          &task.path,
          line_attrs,
        );
      }
    }

    // Bottom help line
    let help = " j/k:navigate  q:quit ";
    let help_attrs = Attrs::default().fg(Color::Idx(245));
    grid.draw_text(Rect::new(0, h - 1, w, 1), help, help_attrs);

    // Send diffs to clients
    for client in &mut self.clients {
      let mut out = String::new();
      client.differ.diff(&mut out, grid).log_ignore();
      let _ = client.sender.send(SrvToClt::Print(out)).await;
      let _ = client.sender.send(SrvToClt::Flush).await;
    }
  }
}

pub fn create_dk_app_task(pc: &TaskContext) -> TaskId {
  pc.add_task(Box::new(|pc| {
    log::debug!("Creating dk app task (id: {})", pc.task_id.0);
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let pc_clone = pc.clone();
    tokio::spawn(async move {
      let app = DkApp {
        pc: pc_clone,
        pr: receiver,
        clients: Vec::new(),
        grid: Grid::new(
          Size {
            width: 80,
            height: 24,
          },
          0,
        ),
        screen_size: Size {
          width: 80,
          height: 24,
        },
        tasks: Vec::new(),
        selected: 0,
      };
      app.run().await;
    });
    TaskInit {
      task: Box::new(ChannelTask::new(sender)),
      stop_on_quit: false,
      status: TaskStatus::Running,
      deps: Vec::new(),
      path: None,
    }
  }))
}
