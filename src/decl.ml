module Caml_unix = Unix

open Core_kernel

type proc = {
  name : string;
  cmd : Cmd.t;
}
[@@deriving show]
type t = proc list [@@deriving show]

let parse str =
  let json = Yojson.Safe.from_string str in
  Yojson.Safe.Util.to_assoc json
  |> List.map ~f:(fun (k, v) ->
         let opts = String.Map.of_alist_exn (Yojson.Safe.Util.to_assoc v) in
         let cmd =
           let shell = Map.find opts "shell" in
           let command =
             match shell with
             | Some cmd -> Cmd.Shell (cmd |> Yojson.Safe.Util.to_string)
             | None ->
                 let cmd =
                   Map.find_exn opts "cmd" |> Yojson.Safe.Util.to_string
                 in
                 let args =
                   Map.find_exn opts "args" |> Yojson.Safe.Util.to_list
                   |> Array.of_list_map ~f:Yojson.Safe.Util.to_string
                 in
                 Args (cmd, args)
           in
           let tty =
             Map.find opts "tty"
             |> Option.value_map ~default:true ~f:Yojson.Safe.Util.to_bool
           in
           {
             Cmd.command;
             env = Some (Caml_unix.environment ());
             cwd = Some (Caml_unix.getcwd ());
             tty;
           }
         in
         { name = k; cmd })

let%expect_test _ =
  let str =
    {|
{
  "htop": {
    "cmd": "htop",
    "args": []
  },
  "top": {
    "cmd": "top",
    "args": []
  }
}
    |}
  in
  let decl = parse str in
  print_endline (show decl);
  [%expect
    {|
    [{ Decl.name = "htop"; cmd = "htop"; args = [] };
      { Decl.name = "top"; cmd = "top"; args = [] }] |}]
