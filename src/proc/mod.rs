pub mod handle;
pub mod msg;

use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, spawn};
use std::time::Duration;

use assert_matches::assert_matches;
use crossterm::event::{MouseButton, MouseEventKind};
use portable_pty::MasterPty;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::spawn_blocking;
use tui::layout::Rect;
use vt100::MouseProtocolMode;

use crate::config::ProcConfig;
use crate::encode_term::{encode_key, encode_mouse_event, KeyCodeEncodeModes};
use crate::error::ResultLogger;
use crate::event::CopyMove;
use crate::key::Key;
use crate::mouse::MouseEvent;

use self::handle::ProcHandle;
use self::msg::{ProcCmd, ProcEvent};

pub struct Inst {
  pub vt: VtWrap,

  pub pid: u32,
  pub master: Box<dyn MasterPty + Send>,
  pub killer: Box<dyn ChildKiller + Send + Sync>,

  pub running: Arc<AtomicBool>,
}

impl Debug for Inst {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Inst")
      .field("pid", &self.pid)
      .field("running", &self.running)
      .finish()
  }
}

pub type VtWrap = Arc<RwLock<vt100::Parser>>;

impl Inst {
  fn spawn(
    id: usize,
    cmd: CommandBuilder,
    tx: UnboundedSender<(usize, ProcEvent)>,
    size: &Size,
  ) -> anyhow::Result<Self> {
    let vt = vt100::Parser::new(size.height, size.width, 1000);
    let vt = Arc::new(RwLock::new(vt));

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
      rows: size.height,
      cols: size.width,
      pixel_width: 0,
      pixel_height: 0,
    })?;

    let running = Arc::new(AtomicBool::new(true));
    let mut child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id().unwrap_or(0);
    let killer = child.clone_killer();

    let _r = tx.send((id, ProcEvent::Started));

    let mut reader = pair.master.try_clone_reader().unwrap();

    {
      let tx = tx.clone();
      let vt = vt.clone();
      let running = running.clone();
      spawn_blocking(move || {
        let mut buf = [0; 4 * 1024];
        loop {
          if !running.load(Ordering::Relaxed) {
            break;
          }

          match reader.read(&mut buf[..]) {
            Ok(count) => {
              if count > 0 {
                if let Ok(mut vt) = vt.write() {
                  vt.process(&buf[..count]);
                  match tx.send((id, ProcEvent::Render)) {
                    Ok(_) => (),
                    Err(_) => break,
                  }
                }
              } else {
                thread::sleep(Duration::from_millis(10));
              }
            }
            _ => break,
          }
        }
      });
    }

    {
      let tx = tx.clone();
      let running = running.clone();
      spawn(move || {
        // Block until program exits
        let exit_code = match child.wait() {
          Ok(status) => status.exit_code(),
          Err(_e) => 1,
        };
        running.store(false, Ordering::Relaxed);
        let _result = tx.send((id, ProcEvent::Stopped(exit_code)));
      });
    }

    let inst = Inst {
      vt,

      pid,
      master: pair.master,
      killer,

      running,
    };
    Ok(inst)
  }

  fn resize(&self, size: &Size) {
    let rows = size.height;
    let cols = size.width;

    self
      .master
      .resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
      })
      .log_ignore();

    if let Ok(mut vt) = self.vt.write() {
      vt.set_size(rows, cols);
    }
  }
}

pub struct Proc {
  pub id: usize,
  pub to_restart: bool,
  pub cmd: CommandBuilder,
  size: Size,

  stop_signal: StopSignal,
  mouse_scroll_speed: usize,

  pub tx: UnboundedSender<(usize, ProcEvent)>,

  pub inst: ProcState,
  pub copy_mode: CopyMode,
}

static NEXT_PROC_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub enum ProcState {
  None,
  Some(Inst),
  Error(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StopSignal {
  #[serde(rename = "SIGINT")]
  SIGINT,
  #[serde(rename = "SIGTERM")]
  SIGTERM,
  #[serde(rename = "SIGKILL")]
  SIGKILL,
  SendKeys(Vec<Key>),
  HardKill,
}

impl Default for StopSignal {
  fn default() -> Self {
    StopSignal::SIGTERM
  }
}

pub fn create_proc(
  name: String,
  cfg: &ProcConfig,
  tx: UnboundedSender<(usize, ProcEvent)>,
  size: Rect,
) -> ProcHandle {
  let proc = Proc::new(cfg, tx, size);
  ProcHandle::from_proc(name, proc)
}

impl Proc {
  pub fn new(
    cfg: &ProcConfig,
    tx: UnboundedSender<(usize, ProcEvent)>,
    size: Rect,
  ) -> Self {
    let id = NEXT_PROC_ID.fetch_add(1, Ordering::Relaxed);
    let size = Size::new(size);
    let mut proc = Proc {
      id,
      to_restart: false,
      cmd: cfg.into(),
      size,

      stop_signal: cfg.stop.clone(),
      mouse_scroll_speed: cfg.mouse_scroll_speed,

      tx,

      inst: ProcState::None,
      copy_mode: CopyMode::None(None),
    };

    if cfg.autostart {
      proc.spawn_new_inst();
    }

    proc
  }

  fn spawn_new_inst(&mut self) {
    assert_matches!(self.inst, ProcState::None);

    let spawned =
      Inst::spawn(self.id, self.cmd.clone(), self.tx.clone(), &self.size);
    let inst = match spawned {
      Ok(inst) => ProcState::Some(inst),
      Err(err) => ProcState::Error(err.to_string()),
    };
    self.inst = inst;
  }

  pub fn start(&mut self) {
    if !self.is_up() {
      self.inst = ProcState::None;
      self.spawn_new_inst();
    }
  }

  fn is_up(&self) -> bool {
    if let ProcState::Some(inst) = &self.inst {
      inst.running.load(Ordering::Relaxed)
    } else {
      false
    }
  }

  pub fn lock_vt(
    &self,
  ) -> Option<std::sync::RwLockReadGuard<'_, vt100::Parser>> {
    match &self.inst {
      ProcState::None => None,
      ProcState::Some(inst) => inst.vt.read().ok(),
      ProcState::Error(_) => None,
    }
  }

  pub fn lock_vt_mut(
    &mut self,
  ) -> Option<std::sync::RwLockWriteGuard<'_, vt100::Parser>> {
    match &self.inst {
      ProcState::None => None,
      ProcState::Some(inst) => inst.vt.write().ok(),
      ProcState::Error(_) => None,
    }
  }

  pub fn kill(&mut self) {
    if self.is_up() {
      if let ProcState::Some(inst) = &mut self.inst {
        let _result = inst.killer.kill();
      }
    }
  }

  #[cfg(not(windows))]
  pub fn stop(&mut self) {
    match self.stop_signal.clone() {
      StopSignal::SIGINT => self.send_signal(libc::SIGINT),
      StopSignal::SIGTERM => self.send_signal(libc::SIGTERM),
      StopSignal::SIGKILL => self.send_signal(libc::SIGKILL),
      StopSignal::SendKeys(keys) => {
        for key in keys {
          self.send_key(&key);
        }
      }
      StopSignal::HardKill => self.kill(),
    }
  }

  #[cfg(windows)]
  pub fn stop(&mut self) {
    match self.stop_signal.clone() {
      StopSignal::SIGINT => log::warn!("SIGINT signal is ignored on Windows"),
      StopSignal::SIGTERM => self.kill(),
      StopSignal::SIGKILL => self.kill(),
      StopSignal::SendKeys(keys) => {
        for key in keys {
          self.send_key(&key);
        }
      }
      StopSignal::HardKill => self.kill(),
    }
  }

  #[cfg(not(windows))]
  fn send_signal(&mut self, sig: libc::c_int) {
    if let ProcState::Some(inst) = &self.inst {
      unsafe { libc::kill(inst.pid as i32, sig) };
    }
  }

  pub fn resize(&mut self, size: Rect) {
    let size = Size::new(size);
    if let ProcState::Some(inst) = &self.inst {
      inst.resize(&size);
    }
    self.size = size;
  }

  pub fn send_key(&mut self, key: &Key) {
    if self.is_up() {
      let application_cursor_keys = self
        .lock_vt()
        .map_or(false, |vt| vt.screen().application_cursor());
      let encoder = encode_key(
        key,
        KeyCodeEncodeModes {
          enable_csi_u_key_encoding: true,
          application_cursor_keys,
          newline_mode: false,
        },
      );
      match encoder {
        Ok(encoder) => {
          self.write_all(encoder.as_bytes());
        }
        Err(_) => {
          log::warn!("Failed to encode key: {}", key.to_string());
        }
      }
    }
  }

  pub fn write_all(&mut self, bytes: &[u8]) {
    if self.is_up() {
      if let Some(mut vt) = self.lock_vt_mut() {
        if vt.screen().scrollback() > 0 {
          vt.set_scrollback(0);
        }
      }
      if let ProcState::Some(inst) = &mut self.inst {
        inst.master.write_all(bytes).log_ignore();
      }
    }
  }

  pub fn scroll_up_lines(&mut self, n: usize) {
    match &mut self.copy_mode {
      CopyMode::None(_) => {
        if let Some(mut vt) = self.lock_vt_mut() {
          Self::scroll_vt_up(&mut vt, n);
        }
      }
      CopyMode::Start(screen, _) | CopyMode::Range(screen, _, _) => {
        Self::scroll_screen_up(screen, n)
      }
    }
  }

  fn scroll_vt_up(vt: &mut vt100::Parser, n: usize) {
    let pos = usize::saturating_add(vt.screen().scrollback(), n);
    vt.set_scrollback(pos);
  }

  fn scroll_screen_up(screen: &mut vt100::Screen, n: usize) {
    let pos = usize::saturating_add(screen.scrollback(), n);
    screen.set_scrollback(pos);
  }

  pub fn scroll_down_lines(&mut self, n: usize) {
    match &mut self.copy_mode {
      CopyMode::None(_) => {
        if let Some(mut vt) = self.lock_vt_mut() {
          Self::scroll_vt_down(&mut vt, n);
        }
      }
      CopyMode::Start(screen, _) | CopyMode::Range(screen, _, _) => {
        Self::scroll_screen_down(screen, n)
      }
    }
  }

  fn scroll_vt_down(vt: &mut vt100::Parser, n: usize) {
    let pos = usize::saturating_sub(vt.screen().scrollback(), n);
    vt.set_scrollback(pos);
  }

  fn scroll_screen_down(screen: &mut vt100::Screen, n: usize) {
    let pos = usize::saturating_sub(screen.scrollback(), n);
    screen.set_scrollback(pos);
  }

  pub fn scroll_half_screen_up(&mut self) {
    self.scroll_up_lines(self.size.height as usize / 2);
  }

  pub fn scroll_half_screen_down(&mut self) {
    self.scroll_down_lines(self.size.height as usize / 2);
  }

  pub fn handle_mouse(&mut self, event: MouseEvent) {
    let copy_mode = match self.copy_mode {
      CopyMode::None(_) => false,
      CopyMode::Start(_, _) | CopyMode::Range(_, _, _) => true,
    };
    let mouse_mode = self
      .lock_vt()
      .map(|vt| vt.screen().mouse_protocol_mode())
      .unwrap_or_default();

    if copy_mode {
      match event.kind {
        MouseEventKind::Down(btn) => match btn {
          MouseButton::Left => {
            let scrollback = match &self.copy_mode {
              CopyMode::None(_) => unreachable!(),
              CopyMode::Start(screen, _) | CopyMode::Range(screen, _, _) => {
                screen.scrollback()
              }
            };
            self.copy_mode =
              CopyMode::None(Some(translate_mouse_pos(&event, scrollback)));
          }
          MouseButton::Right => {
            self.copy_mode = match std::mem::take(&mut self.copy_mode) {
              CopyMode::None(_) => unreachable!(),
              CopyMode::Start(screen, start)
              | CopyMode::Range(screen, start, _) => {
                let pos = translate_mouse_pos(&event, screen.scrollback());
                CopyMode::Range(screen, start, pos)
              }
            };
          }
          MouseButton::Middle => (),
        },
        MouseEventKind::Up(_) => (),
        MouseEventKind::Drag(MouseButton::Left) => {
          self.copy_mode = match std::mem::take(&mut self.copy_mode) {
            CopyMode::None(_) => unreachable!(),
            CopyMode::Start(screen, start)
            | CopyMode::Range(screen, start, _) => {
              let pos = translate_mouse_pos(&event, screen.scrollback());
              CopyMode::Range(screen, start, pos)
            }
          };
        }
        MouseEventKind::Drag(_) => (),
        MouseEventKind::Moved => (),
        MouseEventKind::ScrollDown => match &mut self.copy_mode {
          CopyMode::None(_) => unreachable!(),
          CopyMode::Start(screen, _) | CopyMode::Range(screen, _, _) => {
            Self::scroll_screen_down(screen, self.mouse_scroll_speed);
          }
        },
        MouseEventKind::ScrollUp => match &mut self.copy_mode {
          CopyMode::None(_) => unreachable!(),
          CopyMode::Start(screen, _) | CopyMode::Range(screen, _, _) => {
            Self::scroll_screen_up(screen, self.mouse_scroll_speed);
          }
        },
        MouseEventKind::ScrollLeft => (),
        MouseEventKind::ScrollRight => (),
      }
    } else {
      if let ProcState::Some(inst) = &mut self.inst {
        match mouse_mode {
          MouseProtocolMode::None => match event.kind {
            MouseEventKind::Down(btn) => match btn {
              MouseButton::Left => {
                if let Some(vt) = inst.vt.read().log_get() {
                  self.copy_mode = CopyMode::None(Some(translate_mouse_pos(
                    &event,
                    vt.screen().scrollback(),
                  )));
                }
              }
              MouseButton::Right | MouseButton::Middle => (),
            },
            MouseEventKind::Up(_) => (),
            MouseEventKind::Drag(MouseButton::Left) => {
              if let Some(vt) = inst.vt.read().log_get() {
                let pos = translate_mouse_pos(&event, vt.screen().scrollback());
                self.copy_mode = match std::mem::take(&mut self.copy_mode) {
                  CopyMode::None(pos_) => CopyMode::Range(
                    vt.screen().clone(),
                    pos_.unwrap_or_default(),
                    pos,
                  ),
                  CopyMode::Start(..) | CopyMode::Range(..) => {
                    unreachable!()
                  }
                };
              }
            }
            MouseEventKind::Drag(_) => (),
            MouseEventKind::Moved => (),
            MouseEventKind::ScrollDown => {
              if let Some(mut vt) = inst.vt.write().log_get() {
                Self::scroll_vt_down(&mut vt, self.mouse_scroll_speed);
              }
            }
            MouseEventKind::ScrollUp => {
              if let Some(mut vt) = inst.vt.write().log_get() {
                Self::scroll_vt_up(&mut vt, self.mouse_scroll_speed);
              }
            }
            MouseEventKind::ScrollLeft => (),
            MouseEventKind::ScrollRight => (),
          },
          MouseProtocolMode::Press
          | MouseProtocolMode::PressRelease
          | MouseProtocolMode::ButtonMotion
          | MouseProtocolMode::AnyMotion => {
            let seq = encode_mouse_event(event);
            let _r = inst.master.write_all(seq.as_bytes());
          }
        }
      }
    }
  }
}

impl Proc {
  pub fn handle_cmd(&mut self, cmd: ProcCmd) {
    match cmd {
      ProcCmd::Start => self.start(),
      ProcCmd::Stop => self.stop(),
      ProcCmd::Kill => self.kill(),

      ProcCmd::SendKey(key) => self.send_key(&key),
      ProcCmd::SendMouse(event) => self.handle_mouse(event),

      ProcCmd::ScrollUp => self.scroll_half_screen_up(),
      ProcCmd::ScrollDown => self.scroll_half_screen_down(),
      ProcCmd::ScrollUpLines { n } => self.scroll_up_lines(n),
      ProcCmd::ScrollDownLines { n } => self.scroll_down_lines(n),

      ProcCmd::CopyModeEnter => match &mut self.inst {
        ProcState::None => (),
        ProcState::Some(inst) => {
          let screen = inst.vt.read().unwrap().screen().clone();
          let y = (screen.size().0 - 1) as i32;
          self.copy_mode = CopyMode::Start(screen, Pos { y, x: 0 });
        }
        ProcState::Error(_) => (),
      },
      ProcCmd::CopyModeLeave => {
        self.copy_mode = CopyMode::None(None);
      }
      ProcCmd::CopyModeMove { dir } => match &self.inst {
        ProcState::None => (),
        ProcState::Some(inst) => {
          let vt = inst.vt.read().unwrap();
          let screen = vt.screen();
          match &mut self.copy_mode {
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
      },
      ProcCmd::CopyModeEnd => {
        self.copy_mode = match std::mem::take(&mut self.copy_mode) {
          CopyMode::Start(screen, start) => {
            CopyMode::Range(screen, start.clone(), start)
          }
          other => other,
        };
      }
      ProcCmd::CopyModeCopy => {
        if let CopyMode::Range(screen, start, end) = &self.copy_mode {
          let (low, high) = Pos::to_low_high(start, end);
          let text = screen.get_selected_text(low.x, low.y, high.x, high.y);

          // TODO: send copy event instead
          crate::clipboard::copy(text.as_str());
        }
        self.copy_mode = CopyMode::None(None);
      }

      ProcCmd::Resize { x, y, w, h } => self.resize(Rect {
        x,
        y,
        width: w,
        height: h,
      }),
    }
  }
}

fn translate_mouse_pos(event: &MouseEvent, scrollback: usize) -> Pos {
  Pos {
    y: event.y - scrollback as i32,
    x: event.x,
  }
}

struct Size {
  width: u16,
  height: u16,
}

impl Size {
  fn new(rect: Rect) -> Size {
    Size {
      width: rect.width.max(3),
      height: rect.height.max(3),
    }
  }
}

pub enum CopyMode {
  None(Option<Pos>),
  Start(vt100::Screen, Pos),
  Range(vt100::Screen, Pos, Pos),
}

impl Default for CopyMode {
  fn default() -> Self {
    CopyMode::None(None)
  }
}

#[derive(
  Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize,
)]
pub struct Pos {
  pub y: i32,
  pub x: i32,
}

impl Pos {
  pub fn to_low_high<'a>(a: &'a Self, b: &'a Self) -> (&'a Self, &'a Self) {
    if a.y > b.y {
      return (b, a);
    } else if a.y == b.y && a.x > b.x {
      return (b, a);
    }
    (a, b)
  }

  pub fn within(start: &Self, end: &Self, target: &Self) -> bool {
    let y = target.y;
    let x = target.x;
    let (low, high) = Pos::to_low_high(start, end);

    if y > low.y {
      if y < high.y {
        true
      } else if y == high.y && x <= high.x {
        true
      } else {
        false
      }
    } else if y == low.y {
      if y < high.y {
        x >= low.x
      } else if y == high.y {
        x >= low.x && x <= high.x
      } else {
        false
      }
    } else {
      false
    }
  }
}
