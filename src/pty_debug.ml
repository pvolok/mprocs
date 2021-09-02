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
