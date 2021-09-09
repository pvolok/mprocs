open StdLabels
open Notty
open Nottui

open Lwd_infix

let help _ = I.string "<C-a: Output>" |> Ui.atom |> Lwd.return

let procs (cols, rows) =
  I.tabulate cols rows (fun col row ->
      let uchar =
        match (row, col) with
        (* top left *)
        | 0, 0 -> Uchar.of_int 0x256d
        (* top right *)
        | 0, _ when col = cols - 1 -> Uchar.of_int 0x256e
        (* top *)
        | 0, _ -> Uchar.of_int 0x2500
        (* bottom left *)
        | _, 0 when row = rows - 1 -> Uchar.of_int 0x2570
        (* bottom right *)
        | _, _ when row = rows - 1 && col = cols - 1 -> Uchar.of_int 0x256f
        (* bottom *)
        | _, _ when row = rows - 1 -> Uchar.of_int 0x2500
        (* left *)
        | _, 0 -> Uchar.of_int 0x2502
        (* right *)
        | _, _ when col = cols - 1 -> Uchar.of_int 0x2502
        (* content *)
        | _, _ -> Uchar.of_char '.'
      in
      I.uchar uchar 1 1)
  |> Ui.atom

let vwinsize =
  Lwd.var (Notty_lwt.winsize Lwt_unix.stdout |> Option.value ~default:(11, 11))

(* Let's make use of the fancy let-operators recently added to OCaml *)
open Lwd_infix
let ui procs_ui term_ui =
  Ui_layout.(
    vert (Lwd.get vwinsize)
      [
        ( Fill,
          fun size ->
            hor size
              [
                ( Fixed 30,
                  Ui_frame.make ~title:"Processes"
                    (Lwd.map State.focus' ~f:(State.equal_focus `Procs))
                    procs_ui );
                ( Fill,
                  Ui_frame.make ~title:"Output"
                    (Lwd.map State.focus' ~f:(State.equal_focus `Output))
                    term_ui );
              ] );
        (Fixed 3, Ui_frame.make ~title:"Help" (Lwd.return false) help);
      ])

let run () =
  let quit, quit_u = Lwt.wait () in

  let latest_term_size = ref (0, 0) in
  let on_resize ~w ~h =
    [%log debug "Term resize: %d %d" w h];
    latest_term_size := (w, h);
    Array.iter (Lwd.peek State.procs_var) ~f:(fun proc ->
        match Lwd.peek proc.Proc.kind_var with
        | Simple _ -> ()
        | Vterm pt ->
            let cur = Vterm.getSize pt.vterm in
            if cur.cols <> w || cur.rows <> h then (
              Proc_term.resize ~rows:h ~cols:w pt;
              Vterm.setSize ~size:{ rows = h; cols = w } pt.vterm))
  in

  let procs_ui = Ui_procs.make in
  let term_ui = Ui_term.make ~on_resize in
  let ui =
    Lwd.map (ui procs_ui term_ui)
      ~f:
        (Ui.event_filter (fun event ->
             match (event, Lwd.peek State.focus_var) with
             | `Key (`ASCII 'A', [ `Ctrl ]), `Procs ->
                 Lwd.set State.focus_var `Output;
                 `Handled
             | `Key (`ASCII 'A', [ `Ctrl ]), `Output ->
                 Lwd.set State.focus_var `Procs;
                 `Handled
             | (`Key (`ASCII 'q', []) | `Key (`Escape, [])), `Procs ->
                 [%log info "Quit keybinding pressed. Quitting."];
                 Lwt.wakeup_later quit_u ();
                 `Handled
             | `Key ((`ASCII 'j' | `Arrow `Down), []), `Procs ->
                 let next = Lwd.peek State.selected_var + 1 in
                 let next =
                   if next >= Array.length (Lwd.peek State.procs_var) then 0
                   else next
                 in
                 Lwd.set State.selected_var next;
                 `Handled
             | `Key ((`ASCII 'k' | `Arrow `Up), []), `Procs ->
                 let next = Lwd.peek State.selected_var - 1 in
                 let next =
                   if next < 0 then Array.length (Lwd.peek State.procs_var) - 1
                   else next
                 in
                 Lwd.set State.selected_var next;
                 `Handled
             | `Key (`ASCII 's', []), `Procs ->
                 (match State.get_current_proc () with
                 | Some proc -> Proc.start proc
                 | None -> ());
                 `Handled
             | `Key (`ASCII 'x', []), `Procs ->
                 (match State.get_current_proc () with
                 | Some proc -> Proc.stop proc
                 | None -> ());
                 `Handled
             | `Key key, `Output ->
                 (match State.get_current_proc () with
                 | Some proc -> Proc.send_key proc key
                 | None -> ());
                 `Handled
             | _ -> `Unhandled))
  in
  let rec resize_loop () =
    Lwt.on_success (Notty_lwt.Term.winch ()) (fun () ->
        match Notty_lwt.winsize Lwt_unix.stdout with
        | Some ((w, h) as size) ->
            [%log debug "Set size: %dx%d" w h];
            Lwd.set vwinsize size;
            resize_loop ()
        | None -> ())
  in
  resize_loop ();

  let running = Nottui_lwt.run ~quit ui in

  let start_processes () =
    let w, h = !latest_term_size in
    [%log debug "Init term size: %dx%d" w h];
    let decl = Stdio.In_channel.read_all "mprocs.json" |> Decl.parse in
    let procs =
      Array.of_list decl
      |> Array.map ~f:(fun { Decl.cmd; name } ->
             Proc.create ~cmd ~size:!latest_term_size ~name ())
    in

    Lwd.set State.procs_var procs
  in
  (* Lwd ignores update if happens syncronously after the first render. *)
  Lwt.on_success (Lwt.pause ()) start_processes;

  Lwt_main.run running
