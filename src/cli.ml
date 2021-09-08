open Cmdliner

let run () =
  [%log debug "----------------------------------------"];
  [%log debug "*************** STARTING ***************"];
  [%log debug "----------------------------------------"];

  let config =
    let doc = "Config file." in
    Arg.(
      value & opt string "./mprocs.json"
      & info [ "c"; "config" ] ~docv:"PATH" ~doc)
  in
  let run config = Main.run () in
  let main_t = Term.(const run $ config) in
  let info =
    Term.info "mprocs" ~doc:"run multiple processes" ~exits:Term.default_exits
  in
  Term.exit @@ Term.eval (main_t, info)
