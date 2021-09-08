type t = {
  process : Lwt_process.process_full;
  buffer : string CCDeque.t;
  last_line : Buffer.t;
  on_update : unit Listeners.t;
}

let run (cmd : Cmd.t) =
  let command =
    match cmd.command with
    | Args command -> command
    | Shell command -> Lwt_process.shell command
  in
  let process = Lwt_process.open_process_full command in
  let buffer = CCDeque.create () in
  let last_line = Buffer.create 80 in
  let on_update = Listeners.create () in

  let notify_update_scheduled = ref false in
  let notify_update () =
    if not !notify_update_scheduled then (
      notify_update_scheduled := true;
      Lwt.on_success (Lwt.pause ()) (fun () ->
          notify_update_scheduled := false;
          Listeners.call on_update ()))
  in

  let push_char c =
    match c with
    | '\r' -> ()
    | '\n' ->
        CCDeque.push_front buffer (Buffer.contents last_line);
        Buffer.clear last_line;
        notify_update ()
    | c ->
        Buffer.add_char last_line c;
        notify_update ()
  in
  Lwt_io.read_chars process#stdout
  |> Lwt_stream.iter push_char
  |> (ignore : _ Lwt.t -> unit);
  Lwt_io.read_chars process#stderr
  |> Lwt_stream.iter push_char
  |> (ignore : _ Lwt.t -> unit);

  { process; buffer; last_line; on_update }

let peek_lines t n =
  if n = 0 then []
  else
    let lines = [ Buffer.contents t.last_line ] in
    let rec add_loop acc buf n =
      if n = 0 then acc
      else
        match buf with
        | [] -> acc
        | line :: rest -> add_loop (line :: acc) rest (n - 1)
    in
    let lines = add_loop lines (CCDeque.to_rev_list t.buffer) (n - 1) in
    lines

let lines_count t = CCDeque.length t.buffer + 1

let send_key ps (key : Nottui.Ui.key) =
  let main, _mods = key in
  let send str = Lwt_io.write ps.process#stdin str |> Lwt.ignore_result in
  match main with
  | `ASCII c ->
      let str = Printf.sprintf "%c" c in
      send str
  | `Uchar uc ->
      let buf = Buffer.create 4 in
      Stdlib.Buffer.add_utf_8_uchar buf uc;
      send (Buffer.contents buf)
  | `Enter -> send "\n"
  | `Tab -> send "\t"
  | `Backspace -> send "\x7f"
  | `Escape -> send "\x1b"
  | _ -> ()

let stop ps =
  if Sys.win32 then ps.process#terminate
  else
    let term_timer = Lwt_unix.sleep 5.0 in
    Lwt.on_success term_timer (fun () -> ps.process#kill Sys.sigterm);
    let kill_timer = Lwt_unix.sleep 10. in
    Lwt.on_success kill_timer (fun () -> ps.process#terminate);

    Lwt.on_termination ps.process#status (fun () ->
        Lwt.cancel term_timer;
        Lwt.cancel kill_timer);

    ps.process#kill Sys.sigint
