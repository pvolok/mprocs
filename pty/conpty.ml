type hpcon

type t = {
  pid : int;
  handle : Unix.file_descr;
  stdin : Unix.file_descr;
  stdout : Unix.file_descr;
  hpc : hpcon;
}

module Internal = struct
  external create_process :
    string option (* prog *) ->
    string (* cmdline *) ->
    string option (* env *) ->
    string option (* cwd *) ->
    int * int (* size(rows, cols) *) ->
    t = "conpty_create_process"

  external process_wait_job : Unix.file_descr -> int Lwt_unix.job
    = "conpty_process_wait_job"

  external kill : t -> unit = "conpty_kill"

  external resize : t -> int -> int -> unit = "conpty_resize"
end

let win32_quote arg =
  if String.length arg > 0 && arg.[0] = '\000' then
    String.sub arg 1 (String.length arg - 1)
  else Filename.quote arg

let create_process ?env ?cwd (prog, args) ~rows ~cols =
  let prog = if String.equal prog "" then None else Some prog in
  let cmdline = String.concat " " (List.map win32_quote (Array.to_list args)) in
  let env =
    match env with
    | None -> None
    | Some env ->
        let len =
          Array.fold_left (fun len str -> String.length str + len + 1) 1 env
        in
        let res = Bytes.create len in
        let ofs =
          Array.fold_left
            (fun ofs str ->
              let len = String.length str in
              String.blit str 0 res ofs len;
              Bytes.set res (ofs + len) '\000';
              ofs + len + 1)
            0 env
        in
        Bytes.set res ofs '\000';
        Some (Bytes.unsafe_to_string res)
  in

  Internal.create_process prog cmdline env cwd (rows, cols)

let wait_proc t = Lwt_unix.run_job (Internal.process_wait_job t.handle)

let kill = Internal.kill

let resize t ~rows ~cols = Internal.resize t rows cols
