let render f (area : Tui.Rect.t) =
  Tui.render_block f
    ~style:(Util.block_style (!Tui_state.focus = `Procs))
    "Processes" area;

  let area = Tui.Rect.sub ~l:1 ~t:1 ~r:1 ~b:1 area in
  Array.iteri
    (fun i (proc : Tui_proc.t) ->
      let area = Tui.Rect.sub ~t:i area in
      let area = { area with h = 1 } in

      let prefix = if !Tui_state.selected = i then "* " else "  " in
      let name = prefix ^ proc.name in
      Tui.render_string f name area;

      let () =
        let str, style =
          match proc.state with
          | Stopped _ -> (" DOWN", Tui.Style.make ~fg:Red ())
          | Running _ | Stopping _ -> (" UP", Tui.Style.make ~fg:Green ())
        in
        let area' = Tui.Rect.sub ~l:(area.w - String.length str) area in
        Tui.render_string f ~style str area'
      in

      ())
    !Tui_state.procs
