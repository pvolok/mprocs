use crossterm::event as ev;
use ocaml::{Raw, Value};
use std::ffi::c_void;
use std::io::Write;
use std::ptr::null;
use std::sync::mpsc::channel;
use std::sync::{mpsc::Receiver, Arc, Mutex};
use std::thread;

use crate::event::Event;

lazy_static! {
  static ref RX: Arc<Mutex<Receiver<Option<crate::event::Event>>>> = {
    let (tx, rx) = channel::<Option<crate::event::Event>>();
    thread::spawn(move || loop {
      let event = ev::read().ok();
      let event = event.map(crate::event::from_crossterm);
      tx.send(event).unwrap();
    });

    Arc::new(Mutex::new(rx))
  };
}

#[no_mangle]
pub unsafe extern "C" fn tui_events_read_rs() -> *const Event {
  let event = (*RX).lock().unwrap().recv().unwrap();
  let ptr = match event {
    Some(event) => Box::into_raw(Box::from(event)),
    None => null(),
  };
  {
    let mut file = std::fs::OpenOptions::new()
      .write(true)
      .append(true)
      .open("tde.log")
      .unwrap();
    writeln!(file, "read ptr: {}", ptr as usize).unwrap();
  }
  ptr
}

#[ocaml::func]
pub fn tui_event_unpack(v: Value) -> Option<Event> {
  let ptr = unsafe { v.abstract_ptr_val_mut::<Event>() };
  {
    let mut file = std::fs::OpenOptions::new()
      .write(true)
      .append(true)
      .open("tde.log")
      .unwrap();
    writeln!(file, "unpack ptr: {}", ptr as usize).unwrap();
  }
  let event = if ptr.is_null() {
    None
  } else {
    unsafe { Some(*Box::from_raw(ptr)) }
  };
  event
}
