open Notty
open Nottui

open Lwd_infix

let make size' =
  let$* procs = State.procs'
  and$ selected = State.selected'
  and$ w, h = size' in
  let from = 0 in
  let to_ = min (Array.length procs) h in
  let rec render_line acc i =
    if i >= from then
      let line_image =
        let proc = procs.(i) in
        let$ state = Lwd.get proc.Proc.state_var in
        let right_attr, right_str =
          match state with
          | Proc.Running _ -> (A.(fg green), "UP")
          | Proc.Stopping _ -> (A.(fg yellow), "UP")
          | Proc.Stopped _ -> (A.(fg red), "DOWN")
        in

        let left_str =
          Printf.sprintf " %c %s"
            (if selected = i then '*' else ' ')
            (Proc.name procs.(i))
        in
        let left_max_len = w - 2 - String.length right_str |> max 0 in
        let left_str = Util.trim left_max_len left_str in

        let space =
          String.make
            (max 0 (w - String.length left_str - String.length right_str))
            ' '
        in

        I.hcat
          [
            I.string left_str;
            I.string space;
            I.string ~attr:right_attr right_str;
          ]
        |> Ui.atom
      in
      render_line (line_image :: acc) (i - 1)
    else acc
  in
  let lines = render_line [] (to_ - 1) in
  Lwd_utils.pack Ui.pack_y lines
