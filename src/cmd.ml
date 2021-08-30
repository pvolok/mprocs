type command =
  | Args of (string * string array)
  | Shell of string
[@@deriving show]

type t = {
  command : command;
  env : string array option;
  cwd : string option;
  tty : bool;
}
[@@deriving show]
