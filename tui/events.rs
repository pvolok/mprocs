use crossterm::event::{read, Event};
use ocaml::{Raw, Value};
use std::{ffi::c_void, time::Duration};

#[repr(C)]
pub struct Job(*const c_void);

#[repr(C)]
struct JobData(Option<crate::event::Event>);

#[no_mangle]
pub unsafe extern "C" fn worker_rs1(job: Job) {
    let data = tui_lwt_get_data(job).as_mut().unwrap();

    let event = read().ok();
    let ml_event = event.map(crate::event::from_crossterm);
    data.0 = ml_event;
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
    let job = unsafe { tui_lwt_create_job(worker_rs1, result_rs, Box::into_raw(data)) };
    return job;
}
