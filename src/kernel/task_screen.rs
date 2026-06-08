use bytes::Bytes;
use compact_str::CompactString;
use tokio::sync::mpsc::Sender;

use crate::{
  kernel::{
    copy_mode::{CopyMove, CopyState, Pos},
    kernel_message::{SharedVt, TaskSender},
    task::{TaskCmd, TaskId},
  },
  term::{
    Color, MouseProtocolMode, Parser, Size, VtEvent, Winsize,
    attrs::Attrs,
    encode::encode_mouse_event,
    grid::{Pos as GridPos, Rect},
    mouse::{MouseButton, MouseEvent, MouseEventKind},
  },
};

pub struct TaskScreen {
  task_id: TaskId,
  size: Winsize,
  vt: SharedVt,
  // Per read events buffer. It is cleared in the beginning of each process().
  events_buf: Vec<VtEvent>,

  observers: Vec<TaskScreenObs>,
  next_direct_id: u64,

  copy: Option<CopySession>,
  /// Content cell of the last left mouse-down, so a drag can anchor the
  /// selection there and copy mode is only entered once a drag begins.
  mouse_down: Option<(u16, u16)>,
  /// Lines scrolled per mouse-wheel notch.
  wheel_lines: usize,
}

struct CopySession {
  state: CopyState,
  present: SharedVt,
}

struct TaskScreenObs {
  target: ObsTarget,
}

enum ObsTarget {
  Framed { sender: TaskSender, size: Winsize },
  Direct { id: u64, sink: Sender<Bytes> },
}

pub enum TaskScreenCmd {
  Observe {
    size: Winsize,
    sender: TaskSender,
  },
  Unobserve {
    observer_id: TaskId,
  },
  Resize {
    size: Winsize,
    observer_id: TaskId,
  },

  CopyEnter,
  CopyLeave,
  CopyMove {
    dir: CopyMove,
  },
  CopyBeginSelection,
  /// Scroll the view: the positive `delta` scrolls up into history.
  Scroll {
    delta: i32,
  },
  CopyYank,
  Mouse {
    event: MouseEvent,
  },
}

pub enum FramedScreenNotify {
  ObserveStarted {
    task_id: TaskId,
  },
  Render {
    task_id: TaskId,
  },

  Bell {
    task_id: TaskId,
  },

  CopyPresent {
    task_id: TaskId,
    vt: Option<SharedVt>,
  },
  Yank {
    text: String,
  },
}

pub enum TaskScreenEffect {
  Write(CompactString),
  Resize(Winsize),
}

impl TaskScreen {
  pub fn vt(&self) -> &SharedVt {
    &self.vt
  }

  pub fn new(task_id: TaskId, vt: SharedVt, wheel_lines: usize) -> Self {
    let size = vt.read().unwrap().screen().size();
    TaskScreen {
      task_id,
      size: Winsize {
        x: size.width,
        y: size.height,
        x_px: 0,
        y_px: 0,
      },
      vt,
      events_buf: Vec::new(),
      observers: Vec::new(),
      next_direct_id: 0,
      copy: None,
      mouse_down: None,
      wheel_lines: wheel_lines.max(1),
    }
  }

  fn broadcast(&self, mut make: impl FnMut(TaskId) -> FramedScreenNotify) {
    let task_id = self.task_id;
    for obs in &self.observers {
      match &obs.target {
        ObsTarget::Framed { sender, .. } => {
          sender.send(TaskCmd::msg(make(task_id)))
        }
        ObsTarget::Direct { .. } => {}
      }
    }
  }

  pub async fn process(
    &mut self,
    bytes: &[u8],
    effects: &mut Vec<TaskScreenEffect>,
  ) {
    let bytes = Bytes::copy_from_slice(bytes);

    if let Ok(mut vt) = self.vt.write() {
      vt.screen.process(&bytes, &mut self.events_buf);
    }

    for obs in &self.observers {
      match &obs.target {
        ObsTarget::Framed { sender, .. } => {
          for event in &self.events_buf {
            match event {
              VtEvent::Bell => {
                sender.send(TaskCmd::msg(FramedScreenNotify::Bell {
                  task_id: self.task_id,
                }));
              }
              VtEvent::Reply(_) => (),
            }
          }
          sender.send(TaskCmd::msg(FramedScreenNotify::Render {
            task_id: self.task_id,
          }));
        }
        ObsTarget::Direct { .. } => {}
      }
    }

    for event in self.events_buf.drain(..) {
      match event {
        VtEvent::Bell => (),
        VtEvent::Reply(s) => effects.push(TaskScreenEffect::Write(s)),
      }
    }

    for obs in &self.observers {
      match &obs.target {
        ObsTarget::Direct { sink, .. } => {
          let _ = sink.send(bytes.clone()).await;
        }
        ObsTarget::Framed { .. } => {}
      }
    }
  }

  pub fn handle_cmd(
    &mut self,
    cmd: TaskScreenCmd,
    effects: &mut Vec<TaskScreenEffect>,
  ) {
    match cmd {
      TaskScreenCmd::Observe { size, sender } => {
        sender.send(TaskCmd::msg(FramedScreenNotify::ObserveStarted {
          task_id: self.task_id,
        }));
        // A late joiner during copy mode must also render the presentation.
        if let Some(session) = &self.copy {
          sender.send(TaskCmd::msg(FramedScreenNotify::CopyPresent {
            task_id: self.task_id,
            vt: Some(session.present.clone()),
          }));
        }
        self.observers.push(TaskScreenObs {
          target: ObsTarget::Framed { sender, size },
        });
        self.sync_size(effects);
      }
      TaskScreenCmd::Unobserve { observer_id } => {
        self.observers.retain(|o| match &o.target {
          ObsTarget::Framed { sender, .. } => sender.task_id != observer_id,
          ObsTarget::Direct { .. } => true,
        });
        self.sync_size(effects);
      }
      TaskScreenCmd::Resize { size, observer_id } => {
        let observer = self.observers.iter_mut().find(|o| match &o.target {
          ObsTarget::Framed { sender, .. } => sender.task_id == observer_id,
          ObsTarget::Direct { .. } => false,
        });
        if let Some(observer) = observer {
          if let ObsTarget::Framed { size: obs_size, .. } = &mut observer.target
          {
            *obs_size = size;
          }
        }
        self.sync_size(effects);
        if self.copy.is_some() {
          self.render_present();
          self.broadcast(|task_id| FramedScreenNotify::Render { task_id });
        }
      }

      TaskScreenCmd::CopyEnter => {
        if self.copy.is_some() {
          return;
        }
        let snapshot = match self.vt.read() {
          Ok(parser) => parser.screen().clone(),
          Err(_) => return,
        };
        let present =
          SharedVt::new(Parser::new(self.size.y.max(1), self.size.x.max(1), 0));
        self.copy = Some(CopySession {
          state: CopyState::new(snapshot),
          present: present.clone(),
        });
        self.render_present();
        self.broadcast(|task_id| FramedScreenNotify::CopyPresent {
          task_id,
          vt: Some(present.clone()),
        });
      }
      TaskScreenCmd::CopyLeave => {
        self.leave_copy();
      }
      TaskScreenCmd::CopyMove { dir } => {
        if let Some(session) = &mut self.copy {
          session.state.move_cursor(dir);
          self.render_present();
          self.broadcast(|task_id| FramedScreenNotify::Render { task_id });
        }
      }
      TaskScreenCmd::CopyBeginSelection => {
        if let Some(session) = &mut self.copy {
          session.state.begin_selection();
          self.render_present();
          self.broadcast(|task_id| FramedScreenNotify::Render { task_id });
        }
      }
      TaskScreenCmd::Scroll { delta } => self.scroll(delta),
      TaskScreenCmd::Mouse { event } => {
        self.handle_mouse(event, effects);
      }
      TaskScreenCmd::CopyYank => {
        let text = self.copy.as_ref().and_then(|s| s.state.selected_text());
        if let Some(text) = text {
          self.broadcast(|_task_id| FramedScreenNotify::Yank {
            text: text.clone(),
          });
        }
        self.leave_copy();
      }
    }
  }

  fn leave_copy(&mut self) {
    if self.copy.take().is_some() {
      self.broadcast(|task_id| FramedScreenNotify::CopyPresent {
        task_id,
        vt: None,
      });
    }
  }

  fn handle_mouse(
    &mut self,
    event: MouseEvent,
    effects: &mut Vec<TaskScreenEffect>,
  ) {
    let mouse_mode = self
      .vt
      .read()
      .map(|p| p.screen().mouse_protocol_mode())
      .unwrap_or(MouseProtocolMode::None);

    if mouse_mode != MouseProtocolMode::None {
      let seq = encode_mouse_for_mode(mouse_mode, event);
      if !seq.is_empty() {
        effects.push(TaskScreenEffect::Write(seq.into()));
      }
      return;
    }

    let row = event.y.max(0) as u16;
    let col = event.x.max(0) as u16;
    match event.kind {
      MouseEventKind::Down(MouseButton::Left) => {
        self.mouse_down = Some((row, col));
        // Reposition the anchor if already selecting; a bare click in the
        // terminal does not enter copy mode.
        if let Some(session) = &mut self.copy {
          let pos = session.state.pos_at(row, col);
          session.state.set_anchor(pos);
          self.render_present();
          self.broadcast(|task_id| FramedScreenNotify::Render { task_id });
        }
      }
      MouseEventKind::Drag(MouseButton::Left) => {
        let entered = if self.copy.is_none() {
          let snapshot = match self.vt.read() {
            Ok(parser) => parser.screen().clone(),
            Err(_) => return,
          };
          let present = SharedVt::new(Parser::new(
            self.size.y.max(1),
            self.size.x.max(1),
            0,
          ));
          self.copy = Some(CopySession {
            state: CopyState::new(snapshot),
            present: present.clone(),
          });
          Some(present)
        } else {
          None
        };
        // A fresh drag anchors at the press cell; later drags only extend.
        let anchor = self.mouse_down.unwrap_or((row, col));
        if let Some(session) = &mut self.copy {
          if entered.is_some() {
            let apos = session.state.pos_at(anchor.0, anchor.1);
            session.state.set_anchor(apos);
          }
          let epos = session.state.pos_at(row, col);
          session.state.set_extent(epos);
        }
        self.render_present();
        match entered {
          Some(present) => {
            self.broadcast(|task_id| FramedScreenNotify::CopyPresent {
              task_id,
              vt: Some(present.clone()),
            })
          }
          None => {
            self.broadcast(|task_id| FramedScreenNotify::Render { task_id })
          }
        }
      }
      MouseEventKind::Up(_) => self.mouse_down = None,
      MouseEventKind::ScrollUp => self.scroll(self.wheel_lines as i32),
      MouseEventKind::ScrollDown => self.scroll(-(self.wheel_lines as i32)),
      MouseEventKind::Down(_)
      | MouseEventKind::Drag(_)
      | MouseEventKind::Moved
      | MouseEventKind::ScrollLeft
      | MouseEventKind::ScrollRight => {}
    }
  }

  /// Wheel scroll. Positive `delta` scrolls up into history.
  fn scroll(&mut self, delta: i32) {
    if let Some(session) = &mut self.copy {
      if delta >= 0 {
        session.state.scroll_up(delta as usize);
      } else {
        session.state.scroll_down((-delta) as usize);
      }
      self.render_present();
    } else if let Ok(mut parser) = self.vt.write() {
      if delta >= 0 {
        parser.screen.scroll_screen_up(delta as usize);
      } else {
        parser.screen.scroll_screen_down((-delta) as usize);
      }
    }
    self.broadcast(|task_id| FramedScreenNotify::Render { task_id });
  }

  /// Composes the frozen snapshot (scrolled), the selection highlight, the HUD
  /// badge, and the selection cursor into the `present` surface that observers
  /// render.
  fn render_present(&mut self) {
    let Some(session) = &self.copy else {
      return;
    };
    let copy = &session.state;
    let Ok(mut parser) = session.present.write() else {
      return;
    };
    let size = Size {
      width: self.size.x.max(1),
      height: self.size.y.max(1),
    };
    parser.set_size(size.height, size.width);
    let grid = parser.screen.grid_mut();
    grid.set_scrollback(0);
    grid.erase_all(Attrs::default());

    let snapshot = copy.snapshot();
    let scrollback = copy.scrollback() as i32;
    let start = copy.start();
    let end = copy.end().unwrap_or(start);
    let highlight = Attrs::default().fg(Color::BLACK).bg(Color::CYAN);

    for row in 0..size.height {
      for col in 0..size.width {
        let Some(cell) = snapshot.cell(row, col) else {
          continue;
        };
        let Some(dst) = grid.drawing_cell_mut(GridPos { row, col }) else {
          continue;
        };
        *dst = cell.clone();
        if !cell.has_contents() {
          dst.set_str(" ");
        }
        let target = Pos {
          y: row as i32 - scrollback,
          x: col as i32,
        };
        if Pos::within(start, end, target) {
          dst.set_attrs(highlight);
        }
      }
    }

    // HUD badge in the top-right corner.
    let off = copy.scrollback();
    let label = if off > 0 {
      format!(" COPY -{} ", off)
    } else {
      " COPY ".to_string()
    };
    let width = (label.len() as u16).min(size.width);
    grid.draw_text(
      Rect::new(size.width - width, 0, width, 1),
      &label,
      Attrs::default().fg(Color::BLACK).bg(Color::BRIGHT_YELLOW),
    );

    // Place the cursor at the selection position.
    let cursor = copy.cursor();
    let cy = cursor.y + scrollback;
    if cy >= 0
      && cy < size.height as i32
      && cursor.x >= 0
      && cursor.x < size.width as i32
    {
      grid.set_pos(GridPos {
        row: cy as u16,
        col: cursor.x as u16,
      });
    }
  }

  pub fn add_direct_observer(&mut self, sink: Sender<Bytes>) -> u64 {
    let id = self.next_direct_id;
    self.next_direct_id += 1;
    self.observers.push(TaskScreenObs {
      target: ObsTarget::Direct { id, sink },
    });
    id
  }

  pub fn remove_direct_observer(&mut self, id: u64) {
    self.observers.retain(|o| match &o.target {
      ObsTarget::Direct { id: oid, .. } => *oid != id,
      ObsTarget::Framed { .. } => true,
    });
  }

  pub fn notify_render(&mut self) {
    for obs in &mut self.observers {
      match &obs.target {
        ObsTarget::Framed { sender, .. } => {
          sender.send(TaskCmd::msg(FramedScreenNotify::Render {
            task_id: self.task_id,
          }));
        }
        ObsTarget::Direct { .. } => {}
      }
    }
  }

  pub fn sync_size(&mut self, effects: &mut Vec<TaskScreenEffect>) {
    let mut size = self.size;
    let framed = self.observers.iter().find_map(|o| match &o.target {
      ObsTarget::Framed { size, .. } => Some(*size),
      ObsTarget::Direct { .. } => None,
    });
    if let Some(observer_size) = framed {
      size = observer_size;
    }
    if size != self.size {
      self.size = size;
      effects.push(TaskScreenEffect::Resize(size));
    }
  }
}

fn encode_mouse_for_mode(mode: MouseProtocolMode, event: MouseEvent) -> String {
  match mode {
    MouseProtocolMode::None => String::new(),
    MouseProtocolMode::Press => match event.kind {
      MouseEventKind::Down(_)
      | MouseEventKind::ScrollDown
      | MouseEventKind::ScrollUp
      | MouseEventKind::ScrollLeft
      | MouseEventKind::ScrollRight => encode_mouse_event(event),
      MouseEventKind::Up(_)
      | MouseEventKind::Drag(_)
      | MouseEventKind::Moved => String::new(),
    },
    MouseProtocolMode::PressRelease => match event.kind {
      MouseEventKind::Down(_)
      | MouseEventKind::Up(_)
      | MouseEventKind::ScrollDown
      | MouseEventKind::ScrollUp
      | MouseEventKind::ScrollLeft
      | MouseEventKind::ScrollRight => encode_mouse_event(event),
      MouseEventKind::Drag(_) | MouseEventKind::Moved => String::new(),
    },
    MouseProtocolMode::ButtonMotion => match event.kind {
      MouseEventKind::Down(_)
      | MouseEventKind::Up(_)
      | MouseEventKind::ScrollDown
      | MouseEventKind::Drag(_)
      | MouseEventKind::ScrollUp
      | MouseEventKind::ScrollLeft
      | MouseEventKind::ScrollRight => encode_mouse_event(event),
      MouseEventKind::Moved => String::new(),
    },
    MouseProtocolMode::AnyMotion => encode_mouse_event(event),
  }
}
