pub trait ResultLogger<R> {
  fn log_ignore(&self) -> ();

  fn log_get(self) -> Option<R>;
}

impl<R, E: ToString> ResultLogger<R> for Result<R, E> {
  fn log_ignore(&self) -> () {
    match self {
      Ok(_) => (),
      Err(err) => log::error!("Error: {}", err.to_string()),
    }
  }

  fn log_get(self) -> Option<R> {
    match &self {
      Ok(_) => (),
      Err(err) => log::error!("Error: {}", err.to_string()),
    }
    self.ok()
  }
}
