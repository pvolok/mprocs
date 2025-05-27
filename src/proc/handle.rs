use super::{
  msg::{ProcCmd, ProcEvent},
  CopyMode, Proc, ReplySender,
};

use std::time::Instant;

/// Amount of time a process has to stay up for autorestart to trigger
const RESTART_THRESHOLD_SECONDS: f64 = 1.0;

pub struct ProcHandle {
  id: usize,
  name: String,
  is_up: bool,
  exit_code: Option<u32>,

  pub to_restart: bool,
  pub autorestart: bool,
  last_start: Option<Instant>,
  changed: bool,

  proc: Proc,
}

impl ProcHandle {
  pub fn from_proc(name: String, proc: Proc, autorestart: bool) -> Self {
    Self {
      id: proc.id,
      name,
      is_up: false,
      exit_code: None,
      to_restart: false,
      autorestart,
      last_start: None,
      changed: false,
      proc,
    }
  }

  pub fn send(&mut self, cmd: ProcCmd) {
    self.proc.handle_cmd(cmd)
  }

  pub fn rename(&mut self, name: &str) {
    self.name.replace_range(.., name);
  }

  pub fn id(&self) -> usize {
    self.id
  }

  pub fn exit_code(&self) -> Option<u32> {
    self.exit_code
  }

  pub fn lock_view(&self) -> ProcViewFrame {
    match &self.proc.inst {
      super::ProcState::None => ProcViewFrame::Empty,
      super::ProcState::Some(inst) => inst
        .vt
        .read()
        .map_or(ProcViewFrame::Empty, |vt| ProcViewFrame::Vt(vt)),
      super::ProcState::Error(err) => ProcViewFrame::Err(err),
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn is_up(&self) -> bool {
    self.is_up
  }

  pub fn changed(&self) -> bool {
    self.changed
  }

  pub fn copy_mode(&self) -> &CopyMode {
    &self.proc.copy_mode
  }

  pub fn focus(&mut self) {
    self.changed = false;
  }

  pub fn duplicate(&self) -> Self {
    let proc = self.proc.duplicate();
    Self {
      id: proc.id,
      name: self.name.clone(),
      is_up: false,
      exit_code: None,
      to_restart: false,
      autorestart: self.autorestart,
      last_start: None,
      changed: false,
      proc,
    }
  }
}

impl ProcHandle {
  fn maybe_stopped(&mut self) {
    if self.is_up {
      return;
    }

    let exit_code = self.exit_code().unwrap();
    if self.autorestart && !self.to_restart && exit_code != 0 {
      match self.last_start {
        Some(last_start) => {
          let elapsed_time = Instant::now().duration_since(last_start);
          if elapsed_time.as_secs_f64() > RESTART_THRESHOLD_SECONDS {
            self.to_restart = true;
          }
        }
        None => self.to_restart = true,
      }
    }
    if self.to_restart {
      self.to_restart = false;
      self.send(ProcCmd::Start);
    }
  }

  pub fn handle_event(&mut self, event: ProcEvent, selected: bool) {
    match event {
      ProcEvent::Render => {
        if !selected {
          self.changed = true;
        }
      }
      ProcEvent::Exited(exit_code) => {
        self.proc.handle_exited(exit_code);
        self.exit_code = Some(exit_code);
        self.is_up = self.proc.is_up();

        self.maybe_stopped();
      }
      ProcEvent::StdoutEOF => {
        self.proc.handle_stdout_eof();
        self.is_up = self.proc.is_up();

        self.maybe_stopped();
      }
      ProcEvent::Started => {
        self.last_start = Some(Instant::now());
        self.is_up = true;
      }
      ProcEvent::TermReply(s) => {
        self.send(ProcCmd::SendRaw(s));
      }
    }
  }
}

pub enum ProcViewFrame<'a> {
  Empty,
  Vt(std::sync::RwLockReadGuard<'a, vt100::Parser<ReplySender>>),
  Err(&'a str),
}
