use std::fmt::Debug;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_matches::assert_matches;
use crossterm::event::{MouseButton, MouseEventKind};
use portable_pty::CommandBuilder;
use tokio::sync::mpsc::UnboundedSender;
use tui::layout::Rect;

use crate::config::ProcConfig;
use crate::encode_term::{encode_key, encode_mouse_event, KeyCodeEncodeModes};
use crate::error::ResultLogger;
use crate::event::CopyMove;
use crate::key::Key;
use crate::mouse::MouseEvent;
use crate::vt100;

use super::handle::ProcHandle;
use super::inst::Inst;
use super::msg::{ProcCmd, ProcEvent};
use super::StopSignal;
use super::{CopyMode, Pos, ReplySender, Size};

pub struct Proc {
  pub id: usize,
  pub cmd: CommandBuilder,
  size: Size,

  stop_signal: StopSignal,
  mouse_scroll_speed: usize,
  scrollback_len: usize,

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

pub fn create_proc(
  name: String,
  cfg: &ProcConfig,
  tx: UnboundedSender<(usize, ProcEvent)>,
  size: Rect,
) -> ProcHandle {
  let proc = Proc::new(cfg, tx, size);
  ProcHandle::from_proc(name, proc, cfg.autorestart)
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
      cmd: cfg.into(),
      size,

      stop_signal: cfg.stop.clone(),
      mouse_scroll_speed: cfg.mouse_scroll_speed,
      scrollback_len: cfg.scrollback_len,

      tx,

      inst: ProcState::None,
      copy_mode: CopyMode::None(None),
    };

    if cfg.autostart {
      proc.spawn_new_inst();
    }

    proc
  }

  pub fn duplicate(&self) -> Self {
    let id = NEXT_PROC_ID.fetch_add(1, Ordering::Relaxed);
    let proc = Self {
      id,
      cmd: self.cmd.clone(),
      size: self.size.clone(),

      stop_signal: self.stop_signal.clone(),
      mouse_scroll_speed: self.mouse_scroll_speed,
      scrollback_len: self.scrollback_len,

      tx: self.tx.clone(),

      inst: ProcState::None,
      copy_mode: CopyMode::None(None),
    };
    proc
  }

  fn spawn_new_inst(&mut self) {
    assert_matches!(self.inst, ProcState::None);

    let spawned = Inst::spawn(
      self.id,
      self.cmd.clone(),
      self.tx.clone(),
      &self.size,
      self.scrollback_len,
    );
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

  pub fn handle_exited(&mut self, exit_code: u32) {
    match &mut self.inst {
      ProcState::None => (),
      ProcState::Some(inst) => {
        inst.master = None;
        inst.exit_code = Some(exit_code);
      }
      ProcState::Error(_) => (),
    }
  }

  pub fn handle_stdout_eof(&mut self) {
    match &mut self.inst {
      ProcState::None => (),
      ProcState::Some(inst) => inst.stdout_eof = true,
      ProcState::Error(_) => (),
    }
  }

  pub fn is_up(&self) -> bool {
    if let ProcState::Some(inst) = &self.inst {
      inst.exit_code.is_none() || !inst.stdout_eof
    } else {
      false
    }
  }

  pub fn lock_vt(
    &self,
  ) -> Option<std::sync::RwLockReadGuard<'_, vt100::Parser<ReplySender>>> {
    match &self.inst {
      ProcState::None => None,
      ProcState::Some(inst) => inst.vt.read().ok(),
      ProcState::Error(_) => None,
    }
  }

  pub fn lock_vt_mut(
    &mut self,
  ) -> Option<std::sync::RwLockWriteGuard<'_, vt100::Parser<ReplySender>>> {
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
        inst.writer.write_all(bytes).log_ignore();
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

  fn scroll_vt_up(vt: &mut vt100::Parser<ReplySender>, n: usize) {
    let pos = usize::saturating_add(vt.screen().scrollback(), n);
    vt.set_scrollback(pos);
  }

  fn scroll_screen_up(screen: &mut vt100::Screen<ReplySender>, n: usize) {
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

  fn scroll_vt_down(vt: &mut vt100::Parser<ReplySender>, n: usize) {
    let pos = usize::saturating_sub(vt.screen().scrollback(), n);
    vt.set_scrollback(pos);
  }

  fn scroll_screen_down(screen: &mut vt100::Screen<ReplySender>, n: usize) {
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
          vt100::MouseProtocolMode::None => match event.kind {
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
          vt100::MouseProtocolMode::Press
          | vt100::MouseProtocolMode::PressRelease
          | vt100::MouseProtocolMode::ButtonMotion
          | vt100::MouseProtocolMode::AnyMotion => {
            let seq = encode_mouse_event(event);
            let _r = inst.writer.write_all(seq.as_bytes());
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
      ProcCmd::SendRaw(s) => match &mut self.inst {
        ProcState::None => (),
        ProcState::Some(inst) => {
          let _ = inst.writer.write_all(s.as_bytes());
        }
        ProcState::Error(_) => (),
      },

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
