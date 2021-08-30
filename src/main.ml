open Core_kernel

class root =
  object (_self)
    inherit LTerm_widget.hbox

    val focused_obs = Inc.observe State.focus

    method! cursor_position =
      match Inc.Observer.value focused_obs with
      | Ok (Some focused) ->
          let cursor = focused#cursor_position in
          let cursor =
            match cursor with
            | Some cursor ->
                let alloc = focused#allocation in
                Some
                  {
                    LTerm_geom.row = alloc.row1 + cursor.row;
                    col = alloc.col1 + cursor.col;
                  }
            | None -> None
          in
          cursor
      | _ -> None
  end

class event_filter =
  object (self)
    inherit LTerm_widget.hbox

    initializer
    (* Event are always handled by the parent node and send to currently active
       panes. *)
    self#on_event (function Key _ -> true | _ -> false)
  end

let app quit_ui =
  let root = new root in

  let box = new event_filter in
  root#add box;

  let layout_hor = new LTerm_widget.hbox in
  let layout_vert = new LTerm_widget.vbox in
  let help = new Help_ui.t in
  layout_vert#add layout_hor;
  layout_vert#add ~expand:false help;
  box#add layout_vert;

  let procs_ui = new Procs_ui.t in

  let term_ui = new Term_ui.t in

  let procs_pane = new Pane_ui.t in
  procs_pane#set_title_utf8 "Processes";
  procs_pane#set procs_ui;
  layout_hor#add ~expand:false procs_pane;

  layout_hor#add ~expand:false (new LTerm_widget.vline);

  let term_pane = new Pane_ui.t in
  term_pane#set_title_utf8 "Output";
  term_pane#set term_ui;
  layout_hor#add term_pane;

  let focusables =
    [|
      ((procs_ui :> LTerm_widget.t), Keymap.procs_help);
      ((term_ui :> LTerm_widget.t), Keymap.output_help);
    |]
  in
  let focus_obs = Inc.observe State.focus in
  let focus_next () =
    let i =
      match focus_obs |> Inc.Observer.value with
      | Ok (Some focused) ->
          Array.find_mapi focusables ~f:(fun i (w, _) ->
              if phys_equal focused (w :> LTerm_widget.t) then Some (i + 1)
              else None)
      | _ -> None
    in
    let i = Option.value i ~default:0 in
    let i = if i >= Array.length focusables then 0 else i in
    let i = if i < 0 then Array.length focusables - 1 else i in
    let next_focus, next_help = focusables.(i) in
    Inc.Var.set State.focus_var (Some next_focus);
    Inc.Var.set State.keymap_var next_help;
    Inc.stabilize ()
  in

  focus_next ();

  let quitting = ref false in
  let quit () =
    let procs_obs = Inc.observe State.procs in
    Inc.stabilize ();
    let procs = Inc.Observer.value_exn procs_obs in
    Inc.Observer.disallow_future_use procs_obs;

    let all_finished =
      Lwt_list.map_p
        (fun proc ->
          Proc.stop proc;
          Proc.stopped proc)
        (Array.to_list procs)
    in
    let all_finished = Lwt.bind all_finished (fun _ -> Lwt.pause ()) in
    Lwt.on_success all_finished (fun _ -> quit_ui ())
  in

  let current_event = ref None in
  root#on_event (fun e ->
      let handled = !quitting in
      let handled =
        handled
        ||
        match e with
        | Key { code = Char c; control = true; _ }
          when CamomileLibrary.UChar.(eq c (of_char 'a')) ->
            focus_next ();
            true
        | Key { code = Char c; meta = false; _ }
          when CamomileLibrary.UChar.(eq c (of_char 'q'))
               && Option.value_map
                    (Inc.Var.value State.focus_var)
                    ~default:true
                    ~f:(phys_equal (procs_ui :> LTerm_widget.t)) ->
            quitting := true;
            quit ();
            true
        | _ -> false
      in
      let handled =
        handled
        ||
        match focus_obs |> Inc.Observer.value with
        | Ok (Some focused) ->
            current_event := Some e;
            focused#send_event e;
            true
        | _ -> false
      in
      handled);

  root

let load_decl path =
  let%lwt json = Lwt_io.with_file ~mode:Lwt_io.input path Lwt_io.read in
  let decl = Decl.parse json in
  Lwt.return decl

let run_async ~config () =
  let%lwt decl = load_decl config in
  Inc.Var.set State.decl_var decl;

  let promise, resolver = Lwt.wait () in
  let%lwt term =
    LTerm.create Lwt_unix.stdin Lwt_io.stdin Lwt_unix.stdout Lwt_io.stdout
  in

  (* Set default theme. *)
  (match LTerm.colors term with
  | 256 ->
      [%log info "256 colors terminal."];
      Theme.cur := Theme.default256
  | _ ->
      [%log info "16 colors terminal."];
      Theme.cur := Theme.default16);

  let%lwt () = LTerm.enable_mouse term in

  Lwt.finalize
    (fun () -> LTerm_widget.run term (app (Lwt.wakeup resolver)) promise)
    (fun () -> LTerm.disable_mouse term)

let run ~config () = run_async ~config () |> Lwt_main.run
