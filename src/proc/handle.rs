use super::{
  msg::{ProcCmd, ProcEvent},
  CopyMode, Proc, ProcState,
};

pub struct ProcHandle {
  id: usize,
  name: String,
  is_up: bool,

  pub to_restart: bool,
  changed: bool,

  proc: Proc,
}

impl ProcHandle {
  pub fn from_proc(name: String, proc: Proc) -> Self {
    Self {
      id: proc.id,
      name,
      is_up: false,
      to_restart: false,
      changed: false,
      proc,
    }
  }

  pub fn send(&mut self, cmd: ProcCmd) {
    self.proc.handle_cmd(cmd)
  }

  pub fn rename(&mut self, name: &str) {
    self.name.replace_range(.., &name);
  }

  pub fn id(&self) -> usize {
    self.id
  }

  pub fn lock_view(&self) -> ProcViewFrame {
    match &self.proc.inst {
      ProcState::None => ProcViewFrame::Empty,
      ProcState::Some(inst) => inst
        .vt
        .read()
        .map_or(ProcViewFrame::Empty, |vt| ProcViewFrame::Vt(vt)),
      ProcState::Error(err) => ProcViewFrame::Err(&err),
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

  pub fn handle_event(&mut self, event: ProcEvent, selected: bool) {
    match event {
      ProcEvent::Render => {
        if !selected {
          self.changed = true;
        }
      }
      ProcEvent::Stopped => {
        self.is_up = false;
        if let ProcState::Some(inst) = &self.proc.inst {
          let exit_status = inst.exit_status.lock().unwrap();
          if let Some(_code) = *exit_status {
            // Here you can handle the exit status, for example:
            // - Update the UI with the exit code
            // - Log the exit status
            // - Perform any other necessary actions based on the exit status
          }
        }
        if self.to_restart {
          self.to_restart = false;
          self.send(ProcCmd::Start);
        }
      }
      ProcEvent::Started => {
        self.is_up = true;
      }
    }
  }

  // New method to retrieve the exit status
  pub fn exit_status(&self) -> Option<i32> {
    if let ProcState::Some(inst) = &self.proc.inst {
      let exit_status = inst.exit_status.lock().unwrap();
      *exit_status
    } else {
      None
    }
  }
}

pub enum ProcViewFrame<'a> {
  Empty,
  Vt(std::sync::RwLockReadGuard<'a, vt100::Parser>),
  Err(&'a str),
}
