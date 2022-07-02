use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, spawn};
use std::time::Duration;

use assert_matches::assert_matches;
use crossterm::event::{MouseEvent, MouseEventKind};
use portable_pty::MasterPty;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::spawn_blocking;
use tui::layout::Rect;
use vt100::MouseProtocolMode;

use crate::config::ProcConfig;
use crate::encode_term::{encode_key, encode_mouse_event, KeyCodeEncodeModes};
use crate::key::Key;

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
    tx: UnboundedSender<(usize, ProcUpdate)>,
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
    let pid = child.process_id().unwrap();
    let killer = child.clone_killer();

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
                vt.clone().write().unwrap().process(&buf[..count]);
                match tx.send((id, ProcUpdate::Render)) {
                  Ok(_) => (),
                  Err(_) => break,
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
        let _status = child.wait();
        running.store(false, Ordering::Relaxed);
        let _result = tx.send((id, ProcUpdate::Stopped));
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
      .unwrap();

    self.vt.write().unwrap().set_size(rows, cols);
  }
}

pub struct Proc {
  pub id: usize,
  pub name: String,
  pub to_restart: bool,
  pub changed: bool,
  pub cmd: CommandBuilder,
  size: Size,

  stop_signal: StopSignal,

  pub tx: UnboundedSender<(usize, ProcUpdate)>,

  pub inst: ProcState,
}

static NEXT_PROC_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub enum ProcState {
  None,
  Some(Inst),
  Error(String),
}

#[derive(Debug)]
pub enum ProcUpdate {
  Render,
  Stopped,
  Started,
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

impl Proc {
  pub fn new(
    name: String,
    cfg: &ProcConfig,
    tx: UnboundedSender<(usize, ProcUpdate)>,
    size: Rect,
  ) -> Self {
    let id = NEXT_PROC_ID.fetch_add(1, Ordering::Relaxed);
    let size = Size::new(size);
    let mut proc = Proc {
      id,
      name,
      to_restart: false,
      changed: false,
      cmd: cfg.into(),
      size,

      stop_signal: cfg.stop.clone(),

      tx,

      inst: ProcState::None,
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

      let _res = self.tx.send((self.id, ProcUpdate::Started));
    }
  }

  pub fn is_up(&self) -> bool {
    if let ProcState::Some(inst) = &self.inst {
      inst.running.load(Ordering::Relaxed)
    } else {
      false
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
      let application_cursor_keys = match &self.inst {
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
      if let ProcState::Some(inst) = &mut self.inst {
        {
          let mut vt = inst.vt.write().unwrap();
          if vt.screen().scrollback() > 0 {
            vt.set_scrollback(0);
          }
        }
        inst.master.write_all(bytes).unwrap();
      }
    }
  }

  pub fn scroll_up_lines(&mut self, n: usize) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mut vt = inst.vt.write().unwrap();
      let pos = usize::saturating_add(vt.screen().scrollback(), n);
      vt.set_scrollback(pos);
    }
  }

  pub fn scroll_down_lines(&mut self, n: usize) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mut vt = inst.vt.write().unwrap();
      let pos = usize::saturating_sub(vt.screen().scrollback(), n);
      vt.set_scrollback(pos);
    }
  }

  pub fn scroll_up(&mut self) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mut vt = inst.vt.write().unwrap();
      let pos = usize::saturating_add(
        vt.screen().scrollback(),
        vt.screen().size().0 as usize / 2,
      );
      vt.set_scrollback(pos);
    }
  }

  pub fn scroll_down(&mut self) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mut vt = inst.vt.write().unwrap();
      let pos = usize::saturating_sub(
        vt.screen().scrollback(),
        vt.screen().size().0 as usize / 2,
      );
      vt.set_scrollback(pos);
    }
  }

  pub fn handle_mouse(&mut self, event: MouseEvent, term_area: Rect) {
    if let ProcState::Some(inst) = &mut self.inst {
      let mut vt = inst.vt.write().unwrap();
      match vt.screen().mouse_protocol_mode() {
        MouseProtocolMode::None => match event.kind {
          MouseEventKind::Down(_) => (),
          MouseEventKind::Up(_) => (),
          MouseEventKind::Drag(_) => (),
          MouseEventKind::Moved => (),
          MouseEventKind::ScrollDown => {
            let pos = usize::saturating_sub(vt.screen().scrollback(), 5);
            vt.set_scrollback(pos);
          }
          MouseEventKind::ScrollUp => {
            let pos = usize::saturating_add(vt.screen().scrollback(), 5);
            vt.set_scrollback(pos);
          }
        },
        MouseProtocolMode::Press
        | MouseProtocolMode::PressRelease
        | MouseProtocolMode::ButtonMotion
        | MouseProtocolMode::AnyMotion => {
          let ev = MouseEvent {
            kind: event.kind,
            column: event.column - term_area.x,
            row: event.row - term_area.y,
            modifiers: event.modifiers,
          };
          let seq = encode_mouse_event(ev);
          let _r = inst.master.write_all(seq.as_bytes());
        }
      }
    }
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
