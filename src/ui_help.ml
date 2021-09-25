let render f area =
  Tui.render_block f ~style:(Util.block_style false) "Help" area;

  let inner = Tui.Rect.sub ~l:1 ~t:1 ~r:1 ~b:1 area in

  let items =
    match !State.focus with
    | `Procs ->
        [
          ("q", "Quit");
          ("C-a", "Output");
          ("x", "Kill");
          ("s", "Start");
          ("k", "Up");
          ("j", "Down");
        ]
    | `Term -> [ ("C-a", "Process list") ]
  in
  let s =
    List.map (fun (k, desc) -> "<" ^ k ^ ": " ^ desc ^ ">") items
    |> String.concat "  "
  in
  Tui.render_string f s inner
