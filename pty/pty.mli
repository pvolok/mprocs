type t

val create :
  ?env:string array ->
  ?cwd:string ->
  string * string array ->
  rows:int ->
  cols:int ->
  t

val resize : rows:int -> columns:int -> t -> unit

val get_fd : t -> Unix.file_descr
val get_pid : t -> int
