module Internal = struct
  external forkpty : int -> int -> (Unix.file_descr * int) option
    = "ocaml_pty_fork"
  external ioctl_set_size : Unix.file_descr -> int -> int -> int
    = "ocaml_pty_ioctl_set_size"
end

module Conpty = struct
  type pty
  type pty_handle = {
    pid : int;
    fd : Unix.file_descr;
    fd_in : Unix.file_descr;
    fd_out : Unix.file_descr;
    pty : pty;
  }

  external create_process : string -> string -> pty_handle
    = "conpty_create_process"

  external process_wait_job : Unix.file_descr -> int Lwt_unix.job
    = "conpty_process_wait_job"

  let wait_proc pty = Lwt_unix.run_job (process_wait_job pty.fd)
end

type t = Unix.file_descr * int

let create ?env ?cwd (prog, args) ~rows ~cols =
  let child = Internal.forkpty rows cols in
  match child with
  | Some pty -> pty
  | None -> (
      try
        (match cwd with None -> () | Some dir -> Sys.chdir dir);
        match env with
        | None -> Unix.execvp prog args
        | Some env -> Unix.execvpe prog args env
      with _ -> (* Do not run at_exit hooks *)
                Unix._exit 127)

let get_fd (fd, _) = fd
let get_pid (_, pid) = pid

let resize ~rows ~columns ((master, _) : t) =
  let (_ret : int) = Internal.ioctl_set_size master columns rows in
  ()
