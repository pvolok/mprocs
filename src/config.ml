module Json = Yojson.Safe
module SMap = Map.Make (String)

type proc = {
  name : string;
  cmd : Cmd.t;
}
[@@deriving show]

type t = { procs : proc list } [@@deriving show]

let parse_procs (name, v) =
  let opts = Yojson.Safe.Util.to_assoc v |> List.to_seq |> SMap.of_seq in
  let cmd =
    let shell = SMap.find_opt "shell" opts in
    let command =
      match shell with
      | Some cmd -> Cmd.Shell (cmd |> Yojson.Safe.Util.to_string)
      | None ->
          let cmd = SMap.find "cmd" opts |> Yojson.Safe.Util.to_string in
          let args =
            SMap.find "args" opts |> Yojson.Safe.Util.to_list |> Array.of_list
            |> Array.map Yojson.Safe.Util.to_string
          in
          Args (cmd, args)
    in
    let tty =
      SMap.find_opt "tty" opts
      |> Option.map Yojson.Safe.Util.to_bool
      |> Option.value ~default:true
    in
    { Cmd.command; env = None; cwd = None; tty }
  in
  { name; cmd }

let parse str =
  let json = Yojson.Safe.from_string str in
  let entries = Json.Util.to_assoc json |> List.to_seq |> SMap.of_seq in
  {
    procs =
      List.map parse_procs (SMap.find "procs" entries |> Json.Util.to_assoc);
  }

let%expect_test _ =
  let str =
    {|
{
  "procs": {
    "htop": {
      "cmd": "htop",
      "args": []
    },
    "top": {
      "cmd": "top",
      "args": []
    }
  }
}
    |}
  in
  let decl = parse str in
  print_endline (show decl);
  [%expect
    {|
    { Config.procs =
      [{ Config.name = "htop";
         cmd =
         { Cmd.command = (Cmd.Args ("htop", [||])); env = None; cwd = None;
           tty = true }
         };
        { Config.name = "top";
          cmd =
          { Cmd.command = (Cmd.Args ("top", [||])); env = None; cwd = None;
            tty = true }
          }
        ]
      } |}]
