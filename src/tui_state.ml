let focus : [ `Procs | `Term ] ref = ref `Procs

let term_size = ref (80, 30)
let procs : Tui_proc.t array ref = ref [||]

let selected = ref 0
let next () =
  let index = !selected + 1 in
  let index = if index >= Array.length !procs then 0 else index in
  selected := index
let prev () =
  let index = !selected - 1 in
  let index = if index < 0 then Array.length !procs - 1 else index in
  selected := index

let get_current () =
  let index = !selected in
  if index >= 0 && index < Array.length !procs then Some !procs.(index)
  else None
