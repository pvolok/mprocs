module Internal = struct
  external forkpty : int -> int -> (Unix.file_descr * int) option
    = "ocaml_pty_fork"
  external ioctl_set_size : Unix.file_descr -> int -> int -> int
    = "ocaml_pty_ioctl_set_size"
end

module Conpty = Conpty

type t =
  | Unix of Unix.file_descr * int
  | Win of Conpty.t

let create_unix ?env ?cwd (prog, args) ~rows ~cols =
  let child = Internal.forkpty rows cols in
  match child with
  | Some (fd, pid) -> Unix (fd, pid)
  | None -> (
      try
        (match cwd with None -> () | Some dir -> Sys.chdir dir);
        match env with
        | None -> Unix.execvp prog args
        | Some env -> Unix.execvpe prog args env
      with _ -> (* Do not run at_exit hooks *)
                Unix._exit 127)

let create ?env ?cwd cmd ~rows ~cols =
  if Sys.win32 then Win (Conpty.create_process ?env ?cwd cmd ~rows ~cols)
  else create_unix ?env ?cwd cmd ~rows ~cols

let get_fd_stdin = function
  | Unix (fd, _) -> fd
  | Win handle -> handle.Conpty.stdin

let get_fd_stdout = function
  | Unix (fd, _) -> fd
  | Win handle -> handle.Conpty.stdout

let get_pid = function Unix (_, pid) -> pid | Win conpty -> conpty.Conpty.pid

let wait = function
  | Unix (fd, pid) -> Lwt_unix.waitpid [] pid |> Lwt.map (fun (_, x) -> x)
  | Win handle ->
      Conpty.wait_proc handle |> Lwt.map (fun code -> Unix.WEXITED code)

let kill = function
  | Unix (fd, pid) as pty ->
      let kill_timer = Lwt_unix.sleep 5. in
      Lwt.on_success kill_timer (fun () -> Unix.kill pid Sys.sigkill);

      Lwt.on_termination (wait pty) (fun _ -> Lwt.cancel kill_timer);

      Unix.kill pid Sys.sigterm
  | Win conpty -> Conpty.kill conpty

let resize ~rows ~columns = function
  | Unix (fd, _) ->
      let (_ret : int) = Internal.ioctl_set_size fd columns rows in
      ()
  | Win conpty -> Conpty.resize conpty rows columns
