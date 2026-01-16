use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ProcessSpec {
  pub prog: String,
  pub args: Vec<String>,
  // pub pty: bool,
  pub cwd: Option<String>,
  pub env: BTreeMap<String, Option<String>>,
}

impl ProcessSpec {
  pub fn from_argv(mut argv: Vec<String>) -> ProcessSpec {
    let prog = if !argv.is_empty() {
      argv.remove(0)
    } else {
      String::new()
    };
    ProcessSpec {
      prog,
      args: argv,
      // pty: true,
      cwd: None,
      env: Default::default(),
    }
  }

  pub fn cwd<T: Into<String>>(&mut self, cwd: T) {
    self.cwd = Some(cwd.into());
  }
  pub fn get_cwd(&self) -> &Option<String> {
    &self.cwd
  }

  pub fn env<K: Into<String>, V: Into<String>>(&mut self, k: K, v: V) {
    self.env.insert(k.into(), Some(v.into()));
  }

  pub fn env_remove<K: Into<String>>(&mut self, k: K) {
    self.env.insert(k.into(), None);
  }
}
