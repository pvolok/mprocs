type event_ptr

let read () =
  let event = C.Fn2.tui_events_read_rs () in
  let event = Funcs_stubs2.(event.lwt) in
  event
  |> Lwt.map (fun x ->
         let event = C.Types.Event.of_c x in
         match event with Finished -> None | event -> Some event)
