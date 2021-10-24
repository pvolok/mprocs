open Stdlib.StdLabels

type t = {
  pid : int;
  pty : Pty.t;
  (*fd : Lwt_unix.file_descr;*)
  input : Lwt_io.input_channel;
  output : Lwt_io.output_channel;
  vterm : Vterm.t;
  stopped : Lwt_unix.process_status Lwt.t;
  on_damage : Vterm.Rect.t Listeners.t;
  on_move_cursor : (Vterm.Pos.t * Vterm.Pos.t * bool) Listeners.t;
  on_move_rect : (Vterm.Rect.t * Vterm.Rect.t) Listeners.t;
  on_term_prop : Vterm.TermProp.t Listeners.t;
}

let string_of_row (row : Vterm.ScreenCell.t array) =
  row
  |> Array.map ~f:(fun cell -> cell.Vterm.ScreenCell.char |> Uchar.to_char)
  |> Array.to_seq |> String.of_seq

let run (cmd : Cmd.t) ~size =
  let w, h = size in
  let prog, args =
    match cmd.command with
    | Args (name, args) -> (name, args)
    | Shell cmd -> Lwt_process.shell cmd
  in

  let prog =
    if String.equal prog "" && Array.length args > 0 then args.(0) else prog
  in

  let vterm = Vterm.make ~rows:h ~cols:w in

  let pty = Pty.create ?env:cmd.env (prog, args) ~rows:h ~cols:w in

  let pid = Pty.get_pid pty in
  let input = Lwt_io.of_unix_fd ~mode:Lwt_io.input (Pty.get_fd_stdout pty) in
  let output = Lwt_io.of_unix_fd ~mode:Lwt_io.output (Pty.get_fd_stdin pty) in

  let stopped = Pty.wait pty in

  let (_ : unit Lwt.t) =
    Lwt_io.read_chars input
    |> Lwt_stream.iter (fun c ->
           let str = String.make 1 c in
           let (_ : int) = Vterm.write ~input:str vterm in
           ())
  in
  Vterm.setOutputCallback
    ~onOutput:(fun s -> Lwt_io.write output s |> (ignore : unit Lwt.t -> unit))
    vterm;

  let on_damage = Listeners.create () in
  Vterm.Screen.setDamageCallback
    ~onDamage:(fun rect -> Listeners.call on_damage rect)
    vterm;

  let on_move_cursor = Listeners.create () in
  Vterm.Screen.setMoveCursorCallback
    ~onMoveCursor:(fun pos old_pos visible ->
      Listeners.call on_move_cursor (pos, old_pos, visible))
    vterm;

  let on_move_rect = Listeners.create () in
  Vterm.Screen.setMoveRectCallback
    ~onMoveRect:(fun a b -> Listeners.call on_move_rect (a, b))
    vterm;

  let on_term_prop = Listeners.create () in
  Vterm.Screen.setTermPropCallback
    ~onSetTermProp:(fun prop -> Listeners.call on_term_prop prop)
    vterm;

  let sb_buffer : Vterm.sb_line list ref = ref [] in

  Vterm.Screen.setScrollbackPopCallback
    ~onPopLine:(fun () ->
      match !sb_buffer with
      | [] -> None
      | line :: rest ->
          sb_buffer := rest;
          Some line)
    vterm;

  Vterm.Screen.setScrollbackPushCallback
    ~onPushLine:(fun line -> sb_buffer := line :: !sb_buffer)
    vterm;

  (*Vterm.Screen.setAltScreen ~enabled:true vterm;*)
  Vterm.setUtf8 ~utf8:true vterm;

  {
    pid;
    pty;
    (*fd;*)
    input;
    output;
    vterm;
    stopped;
    on_damage;
    on_move_cursor;
    on_move_rect;
    on_term_prop;
  }

let resize ~rows ~cols pt =
  Vterm.setSize ~size:{ rows; cols } pt.vterm;
  Pty.resize ~rows ~columns:cols pt.pty

let send_key pt (key : Tui.Event.KeyEvent.t) =
  let modifier =
    if key.modifiers.control <> 0 then Vterm.Control
    else if key.modifiers.shift <> 0 then Vterm.Shift
    else if key.modifiers.alt <> 0 then Vterm.Alt
    else Vterm.None
  in

  let send key mod_ = Vterm.Keyboard.input pt.vterm key mod_ in

  match key.code with
  | Char code -> send (Unicode (Uchar.of_int code)) modifier
  | Backspace -> send Vterm.Backspace modifier
  | Esc -> send Vterm.Escape modifier
  | Enter -> send Vterm.Enter modifier
  | Tab -> send Vterm.Tab modifier
  | Up -> send Vterm.Up modifier
  | Down -> send Vterm.Down modifier
  | Left -> send Vterm.Left modifier
  | Right -> send Vterm.Right modifier
  | Insert -> send Vterm.Insert modifier
  | Delete -> send Vterm.Delete modifier
  | Home -> send Vterm.Home modifier
  | End -> send Vterm.End modifier
  | PageDown -> send Vterm.PageDown modifier
  | PageUp -> send Vterm.PageUp modifier
  | _ -> [%log warn "Proc_term.send_key ignored key."]

let stop pt = Pty.kill pt.pty

let stopped pt = pt.stopped
