mod event;
mod events;
mod layout;
mod render;
mod render_widget;
mod style;
mod terminal;

#[macro_use]
extern crate lazy_static;

use std::io::Write;

#[ocaml::func]
pub fn tui_init() {
  ::std::panic::set_hook(Box::new(|info| unsafe {
    let err = info.payload();
    info.location();

    let mut file = std::fs::OpenOptions::new()
      .append(true)
      .create(true)
      .open("mprocs.log")
      .unwrap();
    if let Some(location) = info.location() {
      writeln!(
        &mut file,
        "panic occurred in file '{}' at line {}",
        location.file(),
        location.line(),
      )
      .unwrap();
    } else {
      writeln!(
        &mut file,
        "panic occurred but can't get location information..."
      )
      .unwrap();
    }

    let msg = if err.is::<&str>() {
      err.downcast_ref::<&str>().unwrap()
    } else if err.is::<String>() {
      err.downcast_ref::<String>().unwrap().as_ref()
    } else {
      "rust panic"
    };

    if let Some(err) = ocaml::Value::named("Rust_exception") {
      ocaml::Error::raise_value(err, msg);
    }

    ocaml::Error::raise_failure(msg)
  }));
}
