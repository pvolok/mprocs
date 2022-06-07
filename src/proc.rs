use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, spawn};
use std::time::Duration;

use assert_matches::assert_matches;
use portable_pty::MasterPty;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::spawn_blocking;
use tui::layout::Rect;

use crate::config::ProcConfig;
use crate::encode_term::{encode_key, KeyCodeEncodeModes};
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
  pub fn spawn(
    id: usize,
    cmd: CommandBuilder,
    tx: UnboundedSender<(usize, ProcUpdate)>,
    size: &Rect,
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

  pub fn resize(&self, size: &Rect) {
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
  pub cmd: CommandBuilder,
  pub size: Rect,

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
    let mut proc = Proc {
      id,
      name,
      to_restart: false,
      cmd: cfg.into(),
      size,

      stop_signal: cfg.stop.clone(),

      tx,

      inst: ProcState::None,
    };

    proc.spawn_new_inst();

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
  fn send_signal(&self, sig: libc::c_int) {
    if sig == libc::SIGKILL || sig == libc::SIGTERM {
      if let ProcState::Some(inst) = &mut self.inst {
        let _result = inst.killer.kill();
      }
    }
  }

  #[cfg(not(windows))]
  fn send_signal(&mut self, sig: libc::c_int) {
    if let ProcState::Some(inst) = &self.inst {
      unsafe { libc::kill(inst.pid as i32, sig) };
    }
  }

  pub fn resize(&mut self, size: Rect) {
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
}
