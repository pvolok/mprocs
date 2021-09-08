open Notty
open Nottui

open Lwd_infix

let make size' =
  let$ procs = State.procs' and$ selected = State.selected' and$ w, h = size' in
  let from = 0 in
  let to_ = min (Array.length procs) h in
  let rec render_line acc i =
    if i >= from then
      let line =
        I.strf ~w " %c %s"
          (if selected = i then '*' else ' ')
          (Proc.name procs.(i))
      in
      render_line (line :: acc) (i - 1)
    else acc
  in
  let lines = render_line [] (to_ - 1) in
  I.vcat lines |> Ui.atom
