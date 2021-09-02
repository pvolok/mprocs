open Core_kernel

let lcolor_of_vcolor (color : Vterm.Color.raw) =
  match Vterm.Color.unpack color with
  | DefaultForeground -> None
  | DefaultBackground -> None
  | Rgb (r, g, b) -> Some (LTerm_style.rgb r g b)
  | Index index -> Some (LTerm_style.index index)

let lstyle_of_vcell (cell : Vterm.ScreenCell.t) =
  let vstyle = cell.style in
  let is_italic = Vterm.Style.isItalic vstyle in
  let style =
    LTerm_style.
      {
        bold = (if Vterm.Style.isBold vstyle then Some true else Some false);
        underline =
          (if Vterm.Style.isUnderline vstyle then Some true else Some false);
        blink = None;
        reverse = None;
        foreground = lcolor_of_vcolor cell.fg;
        background = lcolor_of_vcolor cell.bg;
      }
  in
  style

let lchar_of_uchar uchar =
  let code =
    match uchar |> Uchar.to_scalar with 0 -> Char.to_int ' ' | c -> c
  in
  let zed_char =
    try code |> CamomileLibrary.UChar.of_int |> Zed_char.unsafe_of_uChar
    with _ -> Zed_char.of_utf8 "?"
  in
  zed_char

let ldraw_of_vcell (cell : Vterm.ScreenCell.t) =
  let style = lstyle_of_vcell cell in
  let char = lchar_of_uchar cell.char in
  (style, char)

class t =
  object (self)
    inherit LTerm_widget.t "term_ui"

    val mutable cur_kind : Proc.kind option = None

    (*method! can_focus = true*)

    val mutable cursor_pos : LTerm_geom.coord = { row = 0; col = 0 }
    val mutable cursor_show : bool = false

    val mutable rows = 20

    val mutable cols = 50

    val mutable last_resized = LTerm_geom.{ rows = 0; cols = 0 }

    val mutable count = 0

    val mutable scount = 0

    val mutable resize_count = 0
    val mutable log_str = ""

    val mutable focused = false

    initializer
    State.focus
    |> Inc.map ~f:(function
         | Some w -> phys_equal (self :> LTerm_widget.t) w
         | None -> false)
    |> Inc.observe
    |> Inc.Observer.on_update_exn ~f:(fun upd ->
           let f =
             match upd with
             | Initialized f -> f
             | Changed (_, f) -> f
             | Invalidated -> false
           in
           focused <- f;
           self#queue_draw);

    State.current_kind |> Inc.observe
    |> Inc.Observer.on_update_exn ~f:(fun upd ->
           let kind =
             match upd with
             | Initialized kind -> kind
             | Changed (_, kind) -> kind
             | Invalidated -> None
           in
           self#set_kind_opt kind);

    self#on_event (function
      | Key key -> (
          match cur_kind with
          | None -> false
          | Some (Simple ps) ->
              Proc_simple.send_key ps key;
              true
          | Some (Vterm pt) ->
              Proc_term.send_key pt key;
              true)
      | _ -> false);

    Lwt.on_success (Lwt_unix.sleep 0.2) (fun () -> self#update_size_after_start)

    val mutable current_callbacks = []

    method clear_kind =
      List.iter current_callbacks ~f:(fun f -> f ());
      cur_kind <- None;

      self#queue_draw

    method set_kind (kind : Proc.kind) =
      self#clear_kind;

      cur_kind <- Some kind;

      let rm_funcs = ref [] in
      let add listeners f =
        let id = Listeners.add listeners f in
        rm_funcs := (fun () -> Listeners.rem listeners id) :: !rm_funcs
      in

      (match kind with
      | Simple ps -> add ps.on_update (fun () -> self#schedule ())
      | Vterm pt ->
          add pt.on_damage (fun _rect ->
              (*[%log debug "on_damage"];*)
              self#schedule ());
          add pt.on_move_cursor (fun (pos, _old_pos, _visible) ->
              (*Printf.sprintf "%s\n" (Vterm.Pos.toString pos) |> log;*)
              cursor_pos <- { row = pos.row; col = pos.col };
              (*[%log debug "on_move_cursor"];*)
              self#schedule ());
          add pt.on_move_rect (fun (a, b) ->
              [%log
                debug "move %s %s" (Vterm.Rect.toString a)
                  (Vterm.Rect.toString b)]);
          add pt.on_term_prop (fun prop ->
              (match prop with
              | CursorVisible show ->
                  cursor_show <- show;
                  [%log debug "on CursorVisible"];
                  self#schedule ()
              | _ -> ());
              Logs.info (fun m -> m "prop: %s" (Vterm.TermProp.toString prop))));

      current_callbacks <- !rm_funcs;

      self#queue_draw

    method set_kind_opt (kind : Proc.kind option) =
      match kind with
      | Some kind -> self#set_kind kind
      | None -> self#clear_kind

    method! cursor_position = if cursor_show then Some cursor_pos else None

    method! draw (cx : LTerm_draw.context) _ =
      let cx_size = LTerm_draw.size cx in
      let cx_rows = LTerm_geom.rows cx_size in
      let cx_cols = LTerm_geom.cols cx_size in

      match cur_kind with
      | None -> ()
      | Some (Simple ps) ->
          for row = 0 to cx_rows - 1 do
            let line = Proc_simple.line ps (cx_rows - row - 1) in
            let zed_line = Zed_string.of_utf8 line in
            let line_len = Zed_string.length zed_line in
            for col = 0 to cx_cols - 1 do
              let ch =
                if line_len > col then Zed_string.get zed_line col
                else Zed_char.of_utf8 " "
              in
              LTerm_draw.draw_char cx row col ch
            done
          done
      | Some (Vterm pt) ->
          let cur_size = Vterm.getSize pt.vterm in

          if cx_rows <> cur_size.rows || cx_cols <> cur_size.cols then (
            (* TODO: use self#set_allocation? *)
            Proc_term.resize ~rows:cx_rows ~cols:cx_cols pt;
            Vterm.setSize ~size:{ rows = cx_rows; cols = cx_cols } pt.vterm);
          ();

          for row = 0 to cur_size.rows - 1 do
            for col = 0 to cur_size.cols - 1 do
              let cell = Vterm.Screen.getCell ~row ~col pt.vterm in
              let style, char = ldraw_of_vcell cell in
              LTerm_draw.draw_char cx row col ~style char
            done
          done

    method private update_size_after_start =
      (* Conpty ignores resizes if sent to fast after creating the process. See:
         https://github.com/microsoft/terminal/issues/10400 *)
      if Sys.win32 && !State.ui_running then
        match cur_kind with
        | Some (Vterm pt) ->
            let size = LTerm_geom.size_of_rect self#allocation in
            Proc_term.resize ~rows:size.rows ~cols:size.cols pt;
            Vterm.setSize ~size:{ rows = size.rows; cols = size.cols } pt.vterm
        | _ -> ()

    val mutable scheduled = false

    method private render () =
      if !State.ui_running then (
        scheduled <- false;
        self#queue_draw)

    method private schedule () =
      (*[%log debug "SCHEDULE"];*)
      if not scheduled then (
        scount <- scount + 1;
        scheduled <- true;
        Lwt.on_success (Lwt.pause ()) self#render)
  end
