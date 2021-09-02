open Core_kernel
open Inc.Let_syntax
let ( let+ ) = Inc.Let_syntax.( >>| )

let ui_running = ref true

let focus_var : LTerm_widget.t option Inc.Var.t = Inc.Var.create None
let focus = Inc.Var.watch focus_var

let keymap_var : (string * LTerm_key.t) list Inc.Var.t = Inc.Var.create []
let keymap = Inc.Var.watch keymap_var

let decl_var : Decl.t Inc.Var.t = Inc.Var.create []
let select_index_var = Inc.Var.create 0

let defs : Decl.t Inc.t = Inc.Var.watch decl_var

let run (decl : Decl.proc) = Proc.create ~cmd:decl.cmd ~name:decl.name ()

let procs =
  let+ defs = defs in
  Array.of_list_map defs ~f:run

let current_proc =
  Inc.map2 procs (Inc.Var.watch select_index_var) ~f:(fun procs index ->
      if index >= 0 && index < Array.length procs then Some procs.(index)
      else None)

let current_kind =
  current_proc >>= fun current_proc ->
  match current_proc with
  | Some proc -> proc.kind_var |> Inc.Var.watch |> Inc.map ~f:Option.some
  | None -> Inc.const None
