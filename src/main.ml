let run ~config () =
  let input_stream =
    Lwt_stream.from Tui.Events.read |> Lwt_stream.map (fun e -> `Input e)
  in
  let render_stream =
    Lwt_stream.from (fun () ->
        Engine.Schedule.next_render () |> Lwt.map (Fun.const (Some `Render)))
  in
  let quit_stream =
    Lwt_stream.from (fun () -> Engine.quit_p |> Lwt.map (fun () -> Some `Quit))
  in
  let all_stream =
    Lwt_stream.choose [ input_stream; render_stream; quit_stream ]
  in

  let rec loop () =
    (try
       Tui.render_start ();
       let f = () in
       let area = Tui.Render.size f in
       let vparts = Tui.Layout.(vsplit [| Min 0; Length 3 |]) area in
       let hparts =
         Tui.Layout.(hsplit [| Length 30; Percentage 100 |] vparts.(0))
       in

       Ui_procs.render f hparts.(0);

       Tui.render_block f
         ~style:(Util.block_style (!State.focus = `Term))
         "Output" hparts.(1);
       let term_area = Tui.Rect.sub ~l:1 ~t:1 ~r:1 ~b:1 hparts.(1) in

       Ui_term.render f term_area;

       Ui_help.render f vparts.(1);

       Tui.render_end ()
     with ex -> [%log err "Tui.render failed: %s" (Printexc.to_string ex)]);

    let%lwt event = Lwt_stream.get all_stream in
    let result =
      match event with
      | Some (`Input event) -> (
          match event with
          | Key key -> (
              let keymap =
                match !State.focus with
                | `Procs -> Keymap.procs
                | `Term -> Keymap.term
              in
              let action = Keymap.handle keymap key in
              match action with
              | Some action ->
                  (match action with
                  | Keymap.Quit -> Engine.quit ()
                  | Keymap.Select_next -> State.next ()
                  | Keymap.Select_prev -> State.prev ()
                  | Keymap.Kill_proc ->
                      State.get_current () |> Option.iter Proc.stop
                  | Keymap.Start_proc ->
                      State.get_current () |> Option.iter Proc.start
                  | Keymap.Focus_term -> State.focus := `Term
                  | Keymap.Focus_procs -> State.focus := `Procs);
                  `Next
              | None ->
                  (match (!State.focus, State.get_current ()) with
                  | `Term, Some proc -> Proc.send_key proc key
                  | _ -> ());
                  `Next)
          | _ -> `Next)
      | Some `Render -> `Next
      | Some `Quit -> `Quit
      | None ->
          Engine.quit ();
          `Next
    in

    match result with `Next -> loop () | `Quit -> Lwt.return_unit
  in

  let loop_promise = loop () in

  (* Starting processes after the first render so that the processes get correct
     terminal size. *)
  Engine.start ~config;

  let%lwt () = loop_promise in

  Lwt.return_unit

let run ~config () =
  let prog =
    Lwt.finalize
      (fun () ->
        Tui.create ();
        run ~config ())
      (fun () ->
        [%log debug "Stop tui."];
        Tui.destroy () |> Lwt.return)
  in
  Lwt_main.run prog;
  Gc.full_major ()
