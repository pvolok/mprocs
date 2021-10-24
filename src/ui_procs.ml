let render f (area : Tui.Rect.t) =
  Tui.render_block f
    ~style:(Util.block_style (!State.focus = `Procs))
    "Processes" area;

  let area = Tui.Rect.sub ~l:1 ~t:1 ~r:1 ~b:1 area in
  Array.iteri
    (fun i (proc : Proc.t) ->
      let area = Tui.Rect.sub ~t:i area in
      let area = { area with h = 1 } in

      let prefix = if !State.selected = i then "* " else "  " in
      let name = prefix ^ proc.name in
      Tui.render_string f name area;

      let () =
        let str, style =
          match proc.state with
          | Stopped _ ->
              ( " DOWN",
                {
                  Tui.C.Types.Style.fg = Some Red;
                  bg = None;
                  add_modifier = 0;
                  sub_modifier = 0;
                } )
          | Running _ | Stopping _ ->
              ( " UP",
                {
                  Tui.C.Types.Style.fg = Some Green;
                  bg = None;
                  add_modifier = 0;
                  sub_modifier = 0;
                } )
        in
        let area' = Tui.Rect.sub ~l:(area.w - String.length str) area in
        Tui.render_string f ~style str area'
      in

      ())
    !State.procs
