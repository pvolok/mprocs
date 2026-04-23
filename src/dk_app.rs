use tokio::sync::mpsc::UnboundedReceiver;

use crate::mprocs::app::{ClientHandle, ClientId};
use crate::{
  error::ResultLogger,
  kernel::kernel_message::{
    KernelCommand, KernelQuery, KernelQueryResponse, TaskContext,
  },
  kernel::task::{
    TaskCmd, TaskDef, TaskId, TaskNotification, TaskNotify, TaskStatus,
  },
  protocol::{CltToSrv, SrvToClt},
  server::server_message::ServerMessage,
  term::{
    Color, Grid, Size, TermEvent,
    attrs::Attrs,
    grid::Rect,
    key::{KeyCode, KeyEventKind, KeyMods},
    scroll_offset,
  },
};

struct DkTaskEntry {
  id: TaskId,
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
        let msg = match msg.downcast::<ServerMessage>() {
          Ok(server_msg) => return self.handle_server_msg(*server_msg).await,
          Err(msg) => msg,
        };
        if let Ok(n) = msg.downcast::<TaskNotification>() {
          return self.handle_notification(n.from, n.notify);
        }
        false
      }
      _ => false,
    }
  }

  fn handle_notification(&mut self, from: TaskId, notify: TaskNotify) -> bool {
    match notify {
      TaskNotify::Added(path, status) => {
        let path = path
          .map(|p| p.to_string())
          .unwrap_or_else(|| format!("<task:{}>", from.0));
        self.tasks.push(DkTaskEntry {
          id: from,
          path,
          status,
        });
        true
      }
      TaskNotify::Started => {
        if let Some(entry) = self.tasks.iter_mut().find(|t| t.id == from) {
          entry.status = TaskStatus::Running;
        }
        true
      }
      TaskNotify::Stopped(_) => {
        if let Some(entry) = self.tasks.iter_mut().find(|t| t.id == from) {
          entry.status = TaskStatus::Down;
        }
        true
      }
      TaskNotify::Removed => {
        self.tasks.retain(|t| t.id != from);
        if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
          self.selected = self.tasks.len() - 1;
        }
        true
      }
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
          id: t.id,
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

    let area = Rect::new(0, 0, self.screen_size.width, self.screen_size.height);
    if area.width < 4 || area.height < 3 {
      return;
    }

    let (title_row, area) = area.split_h(1);
    let (area, help_row) = area.split_h(area.height - 1);

    // Title bar
    let bar_attrs = Attrs::default()
      .fg(Color::BLACK)
      .bg(Color::WHITE)
      .set_bold(true);
    grid.draw_line(title_row, " dekit", bar_attrs);

    // Task list
    if self.tasks.is_empty() {
      let attrs = Attrs::default().fg(Color::Idx(245));
      grid.draw_text(area.inner((1, 2)), "No tasks", attrs);
    } else {
      let max_rows = area.height as usize;
      let start = scroll_offset(self.selected, self.tasks.len(), max_rows);

      for (i, task) in self.tasks.iter().enumerate().skip(start).take(max_rows)
      {
        let Some(row) = area.row((i - start) as u16) else {
          break;
        };
        let is_selected = i == self.selected;
        let bg = if is_selected {
          Color::Idx(236)
        } else {
          Color::Default
        };

        let (status_col, path_col) = row.inner((0, 1)).split_v(2);

        let (status_char, status_color) = match task.status {
          TaskStatus::Running => ("●", Color::GREEN),
          TaskStatus::Down => ("○", Color::RED),
        };
        grid.draw_line(
          status_col,
          status_char,
          Attrs::default().fg(status_color).bg(bg),
        );
        grid.draw_line(path_col, &task.path, Attrs::default().bg(bg));
      }
    }

    // Bottom help line
    let help_attrs = Attrs::default().fg(Color::Idx(245));
    grid.draw_line(help_row, " j/k:navigate  q:quit", help_attrs);

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
  pc.spawn_async(
    TaskDef {
      status: TaskStatus::Running,
      ..Default::default()
    },
    |pc, receiver| async move {
      log::debug!("Creating dk app task (id: {})", pc.task_id.0);
      let app = DkApp {
        pc,
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
    },
  )
}
