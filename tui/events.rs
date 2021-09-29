use crossterm::event::{read, Event};
use ocaml::{Raw, Value};
use std::io::Write;
use std::{cell::RefCell, io};
use std::{ffi::c_void, time::Duration};

use std::sync::mpsc::channel;
use std::sync::{mpsc::Receiver, Arc, Mutex};
use std::thread;

#[repr(C)]
pub struct Job(*const c_void);

#[repr(C)]
struct JobData(Option<crate::event::Event>);

lazy_static! {
  static ref RX: Arc<Mutex<Receiver<Option<crate::event::Event>>>> = {
    let (tx, rx) = channel::<Option<crate::event::Event>>();
    thread::spawn(move || loop {
      let event = read().ok();
      let event = event.map(crate::event::from_crossterm);
      tx.send(event).unwrap();
    });

    Arc::new(Mutex::new(rx))
  };
}

#[no_mangle]
pub unsafe extern "C" fn worker_rs(job: Job) {
  let data = tui_lwt_get_data(job).as_mut().unwrap();

  let event = (*RX).lock().unwrap().recv().unwrap();
  data.0 = event;
}

#[no_mangle]
pub unsafe extern "C" fn result_rs(job: Job) -> Raw {
  let data = Box::from_raw(tui_lwt_get_data(job));

  ocaml::body!(gc: {
    match data.0 {
      Some(event) => ocaml::Value::some(gc, event).raw(),
      None => ocaml::Value::none().raw(),
    }
  })
}

extern "C" {
  fn tui_lwt_create_job(
    worker: unsafe extern "C" fn(Job),
    result: unsafe extern "C" fn(Job) -> Raw,
    data: *mut JobData,
  ) -> Raw;

  fn tui_lwt_get_data(job: Job) -> *mut JobData;
}

#[ocaml::func]
pub fn tui_events_read_job() -> Raw {
  let data = Box::from(JobData(Option::None));
  let job =
    unsafe { tui_lwt_create_job(worker_rs, result_rs, Box::into_raw(data)) };
  return job;
}
