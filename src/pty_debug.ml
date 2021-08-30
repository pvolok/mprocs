let run_pty () =
  let run () =
    let%lwt () = LTerm.printl "Pty monitor" in
    let%lwt term =
      LTerm.create Lwt_unix.stdin Lwt_io.stdin Lwt_unix.stdout Lwt_io.stdout
    in
    let%lwt () =
      LTerm.printl
        (match LTerm.is_a_tty term with true -> "tty" | false -> "no")
    in
    let%lwt mode = LTerm.enter_raw_mode term in
    let rec loop () =
      match%lwt LTerm.read_event term with
      | LTerm_event.Resize { rows; cols } ->
          let%lwt () = LTerm.printf "Resize: %d:%d\n" rows cols in
          loop ()
      | LTerm_event.Key ({ code = Char uchar; control; meta; shift } as key)
        -> (
          LTerm_key.to_string key |> LTerm.printl
          |> (ignore : unit Lwt.t -> unit);
          match CamomileLibrary.UChar.char_of uchar with
          | 'q' when control -> Lwt.return ()
          | _ -> loop ())
      | _ -> loop ()
    in
    let%lwt () = loop () in
    LTerm.leave_raw_mode term mode
  in
  run () |> Lwt_main.run

let run_ptyfork () =
  (*let cmd = "/bin/echo" in*)
  let cmd = "/Users/pvolok/root/_build/default/bin/tde" in
  let pty = Pty.create (cmd, [| "tde"; "pty" |]) ~rows:20 ~cols:80 in
  let fd = Pty.get_fd pty |> Lwt_unix.of_unix_file_descr in
  let ic = Lwt_io.of_fd ~mode:Lwt_io.input fd in
  let run () =
    let rec loop () =
      try%lwt
        let%lwt ch = Lwt_io.read_char ic in
        print_char ch;
        loop ()
      with End_of_file -> Lwt.return ()
    in
    Lwt.on_success (Lwt_unix.sleep 1.0) (fun () ->
        Pty.resize ~rows:20 ~columns:30 pty);
    Lwt.on_success (Lwt_unix.sleep 2.0) (fun () ->
        Pty.resize ~rows:20 ~columns:40 pty);
    Lwt.on_success (Lwt_unix.sleep 3.0) (fun () ->
        Pty.resize ~rows:20 ~columns:40 pty);
    Lwt.on_success (Lwt_unix.sleep 5.0) (fun () ->
        Lwt_unix.write_string fd "q" 0 1 |> (ignore : int Lwt.t -> unit));
    loop ()
  in
  Lwt_main.run (run ());
  ()
