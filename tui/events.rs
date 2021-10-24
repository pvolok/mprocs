use crossterm::event as ev;
use std::sync::mpsc::channel;
use std::sync::{mpsc::Receiver, Arc, Mutex};
use std::thread;

lazy_static! {
  static ref RX: Arc<Mutex<Receiver<Option<crate::ctypes::types::Event>>>> = {
    let (tx, rx) = channel::<Option<crate::ctypes::types::Event>>();
    thread::spawn(move || loop {
      let event = ev::read().ok();
      let event = event.map(crate::conv::event_to_c);
      tx.send(event).unwrap();
    });

    Arc::new(Mutex::new(rx))
  };
}

#[no_mangle]
pub extern "C" fn tui_events_read() -> crate::ctypes::types::Event {
  let event = (*RX).lock().unwrap().recv().unwrap();
  match event {
    Some(event) => event,
    None => crate::ctypes::types::Event::Finished,
  }
}
