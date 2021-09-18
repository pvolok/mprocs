external read_job : unit -> Event.t option Lwt_unix.job = "tui_events_read_job"

let read () = Lwt_unix.run_job (read_job ())
