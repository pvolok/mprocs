module SMap = Map.Make (String)

type proc = {
  name : string;
  cmd : Cmd.t;
}
[@@deriving show]
type t = proc list [@@deriving show]

let parse str =
  let json = Yojson.Safe.from_string str in
  Yojson.Safe.Util.to_assoc json
  |> List.map (fun (k, v) ->
         let opts = Yojson.Safe.Util.to_assoc v |> List.to_seq |> SMap.of_seq in
         let cmd =
           let shell = SMap.find_opt "shell" opts in
           let command =
             match shell with
             | Some cmd -> Cmd.Shell (cmd |> Yojson.Safe.Util.to_string)
             | None ->
                 let cmd = SMap.find "cmd" opts |> Yojson.Safe.Util.to_string in
                 let args =
                   SMap.find "args" opts |> Yojson.Safe.Util.to_list
                   |> Array.of_list
                   |> Array.map Yojson.Safe.Util.to_string
                 in
                 Args (cmd, args)
           in
           let tty =
             SMap.find_opt "tty" opts
             |> Option.map Yojson.Safe.Util.to_bool
             |> Option.value ~default:true
           in
           {
             Cmd.command;
             env = Some (Unix.environment ());
             cwd = Some (Unix.getcwd ());
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
