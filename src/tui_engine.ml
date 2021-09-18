let started = ref false

module Schedule = struct
  let on_render = Listeners.create ()
  let scheduled = ref false

  let schedule () =
    if not !scheduled then (
      scheduled := true;
      Lwt.on_success (Lwt.pause ()) (fun () ->
          scheduled := false;
          Listeners.call on_render ()))

  let next_render () =
    let p, r = Lwt.wait () in
    Listeners.add_once on_render (Lwt.wakeup_later r)
    |> (ignore : Listeners.id -> unit);
    p
end

let quit_p, quit_r = Lwt.wait ()

let start ~config =
  assert (not !started);

  let load () =
    let%lwt src = Lwt_io.with_file ~mode:Lwt_io.input config Lwt_io.read in
    let decl = Decl.parse src in
    let procs =
      Array.of_list decl
      |> Array.mapi (fun i { Decl.cmd; name } ->
             let proc =
               Tui_proc.create ~cmd ~size:!Tui_state.term_size ~name ()
             in
             let _id : Listeners.id =
               Listeners.add proc.on_rerender (fun () ->
                   if i = !Tui_state.selected then Schedule.schedule ())
             in
             proc)
    in

    Tui_state.procs := procs;
    Schedule.schedule ();
    Lwt.return_unit
  in
  Lwt.on_any (load ())
    (fun () -> [%log debug "Processes started."])
    (fun ex ->
      [%log err "Failed to start processes: %s" (Printexc.to_string ex)]);
  ()

let quit () =
  let all =
    Lwt_list.map_p
      (fun proc ->
        Tui_proc.stop proc;
        Tui_proc.stopped proc)
      (Array.to_list !Tui_state.procs)
  in
  Lwt.on_any all
    (fun _ -> Lwt.wakeup_later quit_r ())
    (fun ex ->
      [%log
        err "Error while waiting for processes to stop: %s"
          (Printexc.to_string ex)];
      Lwt.wakeup_later quit_r ())

let resize_term size =
  Tui_state.term_size := size;

  let w, h = size in
  Array.iter
    (fun proc ->
      match Tui_proc.state proc with
      | Running (Vterm pt) | Stopping (Vterm pt) ->
          Proc_term.resize ~rows:h ~cols:w pt
      | _ -> ())
    !Tui_state.procs
