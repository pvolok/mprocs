type event_ptr

external event_job : unit -> event_ptr Lwt_unix.job = "tui_event_job"

external unpack_event : event_ptr -> Event.t option = "tui_event_unpack"

let read () = Lwt_unix.run_job (event_job ()) |> Lwt.map unpack_event
