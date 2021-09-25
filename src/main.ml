let run ~config () =
  let input_stream =
    Lwt_stream.from Tui.Events.read |> Lwt_stream.map (fun e -> `Input e)
  in
  let render_stream =
    Lwt_stream.from (fun () ->
        Tui_engine.Schedule.next_render () |> Lwt.map (Fun.const (Some `Render)))
  in
  let quit_stream =
    Lwt_stream.from (fun () ->
        Tui_engine.quit_p |> Lwt.map (fun () -> Some `Quit))
  in
  let all_stream =
    Lwt_stream.choose [ input_stream; render_stream; quit_stream ]
  in

  let rec loop () =
    (try
       Tui.render (fun f ->
           try
             let area = Tui.Render.size f in
             let vparts = Tui.Layout.(vsplit [| Min 0; Length 3 |]) area in
             let hparts =
               Tui.Layout.(hsplit [| Length 30; Percentage 100 |] vparts.(0))
             in

             Tui_procs.render f hparts.(0);

             Tui.render_block f
               ~style:(Util.block_style (!Tui_state.focus = `Term))
               "Output" hparts.(1);
             let term_area = Tui.Rect.sub ~l:1 ~t:1 ~r:1 ~b:1 hparts.(1) in

             Tui_term_ui.render f term_area;

             Ui_help.render f vparts.(1)
           with ex -> [%log err "Render error: %s" (Printexc.to_string ex)])
       (*;*)
     with ex -> [%log err "Tui.render failed: %s" (Printexc.to_string ex)]);

    let%lwt event = Lwt_stream.get all_stream in
    let result =
      match event with
      | Some (`Input event) -> (
          match event with
          | Key key -> (
              let keymap =
                match !Tui_state.focus with
                | `Procs -> Keymap.procs
                | `Term -> Keymap.term
              in
              let action = Keymap.handle keymap key in
              match action with
              | Some action ->
                  (match action with
                  | Keymap.Quit -> Tui_engine.quit ()
                  | Keymap.Select_next -> Tui_state.next ()
                  | Keymap.Select_prev -> Tui_state.prev ()
                  | Keymap.Kill_proc ->
                      Tui_state.get_current () |> Option.iter Tui_proc.stop
                  | Keymap.Start_proc ->
                      Tui_state.get_current () |> Option.iter Tui_proc.start
                  | Keymap.Focus_term -> Tui_state.focus := `Term
                  | Keymap.Focus_procs -> Tui_state.focus := `Procs);
                  `Next
              | None ->
                  (match (!Tui_state.focus, Tui_state.get_current ()) with
                  | `Term, Some proc -> Tui_proc.send_key proc key
                  | _ -> ());
                  `Next)
          | _ -> `Next)
      | Some `Render -> `Next
      | Some `Quit -> `Quit
      | None ->
          Tui_engine.quit ();
          `Next
    in

    match result with `Next -> loop () | `Quit -> Lwt.return_unit
  in

  let loop_promise = loop () in

  (* Starting processes after the first render so that the processes get correct
     terminal size. *)
  Tui_engine.start ~config;

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
