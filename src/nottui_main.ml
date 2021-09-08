open StdLabels
open Notty
open Nottui

open Lwd_infix

let frame wfocused f vsize =
  let body = f (Lwd.map vsize ~f:(fun (w, h) -> (w - 2, h - 2))) in
  let body = Lwd.map body ~f:(Ui.shift_area (-1) (-1)) in

  let title = I.string "Frame" |> I.pad ~l:1 in

  let uchr code = I.uchar (Uchar.of_int code) 1 1 in
  let frame =
    let$ w, h = vsize and$ focused = wfocused in
    let border =
      I.tabulate w h (fun x y ->
          match (x, y) with
          (* top left *)
          | 0, 0 -> uchr 0x256d
          (* top right *)
          | _, 0 when x = w - 1 -> uchr 0x256e
          (* top *)
          | _, 0 -> uchr 0x2500
          (* bottom left *)
          | 0, _ when y = h - 1 -> uchr 0x2570
          (* bottom right *)
          | _, _ when y = h - 1 && x = w - 1 -> uchr 0x256f
          (* bottom *)
          | _, _ when y = h - 1 -> uchr 0x2500
          (* left *)
          | 0, _ -> uchr 0x2502
          (* right *)
          | _, _ when x = w - 1 -> uchr 0x2502
          (* content *)
          | _, _ -> I.void 1 1)
    in
    let border = if focused then I.attr A.(fg green) border else border in
    Ui.atom I.(title </> border)
  in

  Lwd_utils.pack Ui.pack_z [ frame; body ]

let help _ = I.string "<C-a: Output>" |> Ui.atom |> Lwd.return

module Layout = struct
  type constr =
    | Fixed of int
    | Fill

  let calc_layout len parts =
    let for_fixed, fill_count =
      List.fold_left parts ~init:(0, 0)
        ~f:(fun (for_fixed, fill_count) (constr, _) ->
          match constr with
          | Fixed n -> (for_fixed + n, fill_count)
          | Fill -> (for_fixed, fill_count + 1))
    in
    let per_fill = (len - for_fixed) / fill_count in
    let _, acc =
      List.fold_left parts ~init:(len, []) ~f:(fun (len, acc) (constr, ui) ->
          let part_len = match constr with Fixed n -> n | Fill -> per_fill in
          (len - part_len, (part_len, ui) :: acc))
    in
    List.rev acc

  let hor (wsize : (int * int) Lwd.t) parts =
    let wsizes =
      let$ w, h = wsize in
      calc_layout w parts |> Array.of_list
      |> Array.map ~f:(fun (l, _) -> (l, h))
    in
    let parts =
      List.mapi parts ~f:(fun i (_, part) ->
          part (Lwd.map wsizes ~f:(fun sizes -> sizes.(i))))
    in
    Lwd_utils.pack Ui.pack_x parts

  let vert (wsize : (int * int) Lwd.t) parts =
    let wsizes =
      let$ w, h = wsize in
      calc_layout h parts |> Array.of_list
      |> Array.map ~f:(fun (l, _) -> (w, l))
    in
    let parts =
      List.mapi parts ~f:(fun i (_, part) ->
          part (Lwd.map wsizes ~f:(fun sizes -> sizes.(i))))
    in
    Lwd_utils.pack Ui.pack_y parts
end

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

let render_term vt (w, h) =
  let ret =
    I.tabulate w h (fun x y ->
        let cell = Vterm.Screen.getCell ~row:y ~col:x vt in
        if Uchar.is_char cell.char && Uchar.to_int cell.char <> 0 then
          I.uchar cell.char 1 1
        else if Uchar.to_int cell.char = 0 then I.char ' ' 1 1
        else I.char '~' 1 1)
  in
  ret

let term ~on_resize size' =
  let vtick = Lwd.var 0 in
  let tick = Lwd.get vtick in

  let scheduled = ref false in
  let schedule () =
    if not !scheduled then (
      scheduled := true;
      Lwt.on_success (Lwt.pause ()) (fun () ->
          scheduled := false;
          vtick $= Lwd.peek vtick + 1))
  in

  let cur_dispose = ref Dispose.empty in
  let proc' =
    Lwd.map State.current' ~f:(fun proc ->
        Dispose.dispose !cur_dispose;
        cur_dispose := Dispose.empty;

        (match proc with
        | Some proc -> (
            match Lwd.peek proc.kind_var with
            | Simple _ -> ()
            | Vterm pt ->
                let dispose = Dispose.empty in

                let dispose =
                  Listeners.addl pt.Proc_term.on_damage
                    (fun _rect -> schedule ())
                    dispose
                in

                cur_dispose := dispose)
        | None -> ());
        proc)
  in

  let last_size = ref (0, 0) in
  let on_resize w h =
    let w0, h0 = !last_size in
    if w0 <> w || h0 <> h then (
      last_size := (w, h);
      on_resize ~w ~h)
  in

  let$ w, h = size' and$ proc = proc' and$ frame_id = tick in
  [%log debug "Term frame %d (%dx%d)" frame_id w h];
  on_resize w h;
  (match proc with
  | Some proc -> (
      match Lwd.peek proc.kind_var with
      | Simple _ -> I.char '?' w h
      | Vterm pt -> render_term pt.Proc_term.vterm (w, h))
  | None -> I.void w h)
  |> Ui.atom

let vwinsize =
  Lwd.var (Notty_lwt.winsize Lwt_unix.stdout |> Option.value ~default:(11, 11))

(* Let's make use of the fancy let-operators recently added to OCaml *)
open Lwd_infix
let ui procs_ui term_ui =
  Layout.(
    vert (Lwd.get vwinsize)
      [
        ( Fill,
          fun size ->
            hor size
              [
                ( Fixed 30,
                  frame
                    (Lwd.map State.focus' ~f:(State.equal_focus `Procs))
                    procs_ui );
                ( Fill,
                  frame
                    (Lwd.map State.focus' ~f:(State.equal_focus `Output))
                    term_ui );
              ] );
        (Fixed 3, frame (Lwd.return false) help);
      ])

let run () =
  let quit, quit_u = Lwt.wait () in

  let () =
    let decl = Stdio.In_channel.read_all "mprocs.json" |> Decl.parse in
    let procs =
      Array.of_list decl
      |> Array.map ~f:(fun { Decl.cmd; name } -> Proc.create ~cmd ~name ())
    in

    Lwd.set State.procs_var procs
  in

  let on_resize ~w ~h =
    [%log debug "Term resize: %d %d" w h];
    Array.iter (Lwd.peek State.procs_var) ~f:(fun proc ->
        match Lwd.peek proc.Proc.kind_var with
        | Simple _ -> ()
        | Vterm pt ->
            let cur = Vterm.getSize pt.vterm in
            if cur.cols <> w || cur.rows <> h then (
              Proc_term.resize ~rows:h ~cols:w pt;
              Vterm.setSize ~size:{ rows = h; cols = w } pt.vterm))
  in
  let procs_ui = W_procs.make in
  let term_ui = term ~on_resize in
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
             | `Key (`ASCII 'j', []), `Procs ->
                 let next = Lwd.peek State.selected_var + 1 in
                 let next =
                   if next >= Array.length (Lwd.peek State.procs_var) then 0
                   else next
                 in
                 Lwd.set State.selected_var next;
                 `Handled
             | `Key (`ASCII 'k', []), `Procs ->
                 let next = Lwd.peek State.selected_var - 1 in
                 let next =
                   if next < 0 then Array.length (Lwd.peek State.procs_var) - 1
                   else next
                 in
                 Lwd.set State.selected_var next;
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
  Lwt_main.run running
