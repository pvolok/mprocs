mod app;
mod event;
mod proc;
mod state;
mod theme;
mod ui_procs;
mod ui_term;

use std::io;

use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), io::Error> {
  let app = App::new();
  app.run().await
}
