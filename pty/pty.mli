type t

val create :
  ?env:string array ->
  ?cwd:string ->
  string * string array ->
  rows:int ->
  cols:int ->
  t

val get_fd_stdin : t -> Unix.file_descr
val get_fd_stdout : t -> Unix.file_descr
val get_pid : t -> int

val wait : t -> Unix.process_status Lwt.t

val kill : t -> unit

val resize : rows:int -> columns:int -> t -> unit
