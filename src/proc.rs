use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, spawn};
use std::time::Duration;

use assert_matches::assert_matches;
use portable_pty::MasterPty;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use tokio::sync::mpsc::Sender;
use tokio::task::spawn_blocking;
use tui::layout::Rect;

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
    tx: Sender<(usize, ProcUpdate)>,
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
                match tx.blocking_send((id, ProcUpdate::Render)) {
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
        let _result = tx.blocking_send((id, ProcUpdate::Stopped));
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
  pub cmd: CommandBuilder,
  pub size: Rect,

  pub tx: Sender<(usize, ProcUpdate)>,

  pub inst: ProcState,
}

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

impl Proc {
  pub fn new(
    id: usize,
    name: String,
    cmd: CommandBuilder,
    tx: Sender<(usize, ProcUpdate)>,
    size: Rect,
  ) -> Self {
    let mut proc = Proc {
      id,
      name,
      cmd,
      size,

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

      let _res = self.tx.try_send((self.id, ProcUpdate::Started));
    }
  }

  pub fn is_up(&self) -> bool {
    if let ProcState::Some(inst) = &self.inst {
      inst.running.load(Ordering::Relaxed)
    } else {
      false
    }
  }

  pub fn term(&mut self) {
    if self.is_up() {
      if let ProcState::Some(inst) = &mut self.inst {
        Self::term_impl(inst);
      }
    }
  }

  #[cfg(windows)]
  fn term_impl(inst: &mut Inst) {
    let _result = inst.killer.kill();
  }

  #[cfg(not(windows))]
  fn term_impl(inst: &mut Inst) {
    unsafe { libc::kill(inst.pid as i32, libc::SIGTERM) };
  }

  pub fn kill(&mut self) {
    if self.is_up() {
      if let ProcState::Some(inst) = &mut self.inst {
        let _result = inst.killer.kill();
      }
    }
  }

  pub fn resize(&mut self, size: Rect) {
    if let ProcState::Some(inst) = &self.inst {
      inst.resize(&size);
    }
    self.size = size;
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
