use std::{fs::File, io::BufReader};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use indexmap::IndexMap;
use serde_yaml::Value;

use crate::{
  event::{AppEvent, CopyMove},
  key::Key,
  keymap::Keymap,
  yaml_val::{value_to_string, Val},
};

#[derive(Debug)]
pub struct Settings {
  keymap_procs: IndexMap<Key, AppEvent>,
  keymap_term: IndexMap<Key, AppEvent>,
  keymap_copy: IndexMap<Key, AppEvent>,
  pub hide_keymap_window: bool,
}

impl Default for Settings {
  fn default() -> Self {
    let mut settings = Self {
      keymap_procs: Default::default(),
      keymap_term: Default::default(),
      keymap_copy: Default::default(),
      hide_keymap_window: false,
    };
    settings.add_defaults();
    settings
  }
}

impl Settings {
  pub fn merge_from_xdg(&mut self) -> Result<()> {
    let path = self.get_xdg_config_path()?;
    match File::open(path) {
      Ok(file) => {
        let reader = BufReader::new(file);
        let settings_value: Value = serde_yaml::from_reader(reader)?;
        let settings_val = Val::new(&settings_value)?;
        self.merge_value(settings_val)?;
      }
      Err(err) => match err.kind() {
        std::io::ErrorKind::NotFound => (),
        _ => return Err(err.into()),
      },
    }

    Ok(())
  }

  #[cfg(windows)]
  fn get_xdg_config_path(&self) -> Result<std::path::PathBuf> {
    let mut path = std::path::PathBuf::from(std::env::var("LOCALAPPDATA")?);
    path.push("mprocs/mprocs.yaml");
    Ok(path)
  }

  #[cfg(not(windows))]
  fn get_xdg_config_path(&self) -> Result<std::path::PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("mprocs")?;
    let path = xdg_dirs.get_config_file("mprocs.yaml");
    Ok(path)
  }

  pub fn merge_value(&mut self, val: Val) -> Result<()> {
    let obj = val.as_object()?;

    fn add_keys<'a>(
      into: &mut IndexMap<Key, AppEvent>,
      val: Option<&'a Val>,
    ) -> Result<()> {
      if let Some(keymap) = val {
        let mut keymap = keymap.as_object()?;

        if let Some(reset) = keymap.shift_remove(&Value::from("reset")) {
          if reset.as_bool()? {
            into.clear();
          }
        }

        for (key, event) in keymap {
          let key = Key::parse(value_to_string(&key)?.as_str())?;
          if event.raw().is_null() {
            into.shift_remove(&key);
          } else {
            let event: AppEvent = serde_yaml::from_value(event.raw().clone())?;
            into.insert(key, event);
          }
        }
      }
      Ok(())
    }
    add_keys(
      &mut self.keymap_procs,
      obj.get(&Value::from("keymap_procs")),
    )?;
    add_keys(&mut self.keymap_term, obj.get(&Value::from("keymap_term")))?;

    if let Some(hide_keymap_window) =
      obj.get(&Value::from("hide_keymap_window"))
    {
      self.hide_keymap_window = hide_keymap_window.as_bool()?;
    }

    Ok(())
  }

  pub fn add_defaults(&mut self) {
    let s = self;

    s.keymap_add_p(
      Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleFocus,
    );
    s.keymap_add_t(
      Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleFocus,
    );
    s.keymap_add_c(
      Key::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
      AppEvent::ToggleFocus,
    );

    s.keymap_add_p(KeyCode::Char('q').into(), AppEvent::QuitOrAsk);
    s.keymap_add_p(KeyCode::Char('Q').into(), AppEvent::ForceQuit);
    s.keymap_add_p(
      Key::new(KeyCode::Down, KeyModifiers::NONE),
      AppEvent::NextProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('j'), KeyModifiers::NONE),
      AppEvent::NextProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Up, KeyModifiers::NONE),
      AppEvent::PrevProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('k'), KeyModifiers::NONE),
      AppEvent::PrevProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('s'), KeyModifiers::NONE),
      AppEvent::StartProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('x'), KeyModifiers::NONE),
      AppEvent::TermProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
      AppEvent::KillProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('r'), KeyModifiers::NONE),
      AppEvent::RestartProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('R'), KeyModifiers::SHIFT),
      AppEvent::ForceRestartProc,
    );
    let ctrlc = Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    s.keymap_add_p(ctrlc, AppEvent::SendKey { key: ctrlc });
    s.keymap_add_p(
      Key::new(KeyCode::Char('a'), KeyModifiers::NONE),
      AppEvent::ShowAddProc,
    );
    s.keymap_add_p(
      Key::new(KeyCode::Char('d'), KeyModifiers::NONE),
      AppEvent::ShowRemoveProc,
    );

    // Scrolling in TERM and COPY modes
    for map in [&mut s.keymap_procs, &mut s.keymap_copy] {
      map.insert(
        Key::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
        AppEvent::ScrollUpLines { n: 3 },
      );
      map.insert(
        Key::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
        AppEvent::ScrollDownLines { n: 3 },
      );
      let ctrlu = Key::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
      map.insert(ctrlu, AppEvent::ScrollUp);
      map.insert(
        Key::new(KeyCode::PageUp, KeyModifiers::NONE),
        AppEvent::ScrollUp,
      );
      let ctrld = Key::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
      map.insert(ctrld, AppEvent::ScrollDown);
      map.insert(
        Key::new(KeyCode::PageDown, KeyModifiers::NONE),
        AppEvent::ScrollDown,
      );
    }

    s.keymap_add_p(
      Key::new(KeyCode::Char('z'), KeyModifiers::NONE),
      AppEvent::Zoom,
    );

    s.keymap_add_p(
      Key::new(KeyCode::Char('v'), KeyModifiers::NONE),
      AppEvent::CopyModeEnter,
    );

    for i in 0..8 {
      let char = char::from_digit(i + 1, 10).unwrap();
      s.keymap_add_p(
        Key::new(KeyCode::Char(char), KeyModifiers::ALT),
        AppEvent::SelectProc { index: i as usize },
      );
    }

    s.keymap_add_c(KeyCode::Esc.into(), AppEvent::CopyModeLeave);
    s.keymap_add_c(KeyCode::Char('v').into(), AppEvent::CopyModeEnd);
    s.keymap_add_c(KeyCode::Char('c').into(), AppEvent::CopyModeCopy);
    for code in [KeyCode::Up, KeyCode::Char('k')] {
      s.keymap_add_c(code.into(), AppEvent::CopyModeMove(CopyMove::Up));
    }
    for code in [KeyCode::Right, KeyCode::Char('l')] {
      s.keymap_add_c(code.into(), AppEvent::CopyModeMove(CopyMove::Right));
    }
    for code in [KeyCode::Down, KeyCode::Char('j')] {
      s.keymap_add_c(code.into(), AppEvent::CopyModeMove(CopyMove::Down));
    }
    for code in [KeyCode::Left, KeyCode::Char('h')] {
      s.keymap_add_c(code.into(), AppEvent::CopyModeMove(CopyMove::Left));
    }
  }

  fn keymap_add_p(&mut self, key: Key, event: AppEvent) {
    self.keymap_procs.insert(key, event);
  }

  fn keymap_add_t(&mut self, key: Key, event: AppEvent) {
    self.keymap_term.insert(key, event);
  }

  fn keymap_add_c(&mut self, key: Key, event: AppEvent) {
    self.keymap_copy.insert(key, event);
  }

  pub fn add_to_keymap(&self, keymap: &mut Keymap) -> Result<()> {
    for (key, event) in &self.keymap_procs {
      keymap.bind_p(key.clone(), event.clone());
    }
    for (key, event) in &self.keymap_term {
      keymap.bind_t(key.clone(), event.clone());
    }
    for (key, event) in &self.keymap_copy {
      keymap.bind_c(key.clone(), event.clone());
    }

    Ok(())
  }
}
