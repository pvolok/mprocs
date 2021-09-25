module Simple = struct
  let render f ps (area : Tui.Rect.t) =
    let lines = Proc_simple.peek_lines ps area.h in
    List.iteri
      (fun i line ->
        let r = Tui.Rect.{ x = area.x; y = area.y + i; w = area.w; h = 1 } in
        Tui.render_string f line r)
      lines
end

module Vterm = struct
  let conv_mod (s : Vterm.Style.t) : Tui.Style.Mod.t =
    (if Vterm.Style.isBold s then Tui.Style.Mod.bold else 0)
    lor (if Vterm.Style.isItalic s then Tui.Style.Mod.italic else 0)
    lor if Vterm.Style.isUnderline s then Tui.Style.Mod.underlined else 0

  let conv_color (c : Vterm.Color.raw) : Tui.Style.color option =
    match Vterm.Color.unpack c with
    | Vterm.Color.DefaultForeground | Vterm.Color.DefaultBackground ->
        Some Tui.Style.Reset
    | Vterm.Color.Rgb (r, g, b) -> Some (Tui.Style.Rgb (r, g, b))
    | Vterm.Color.Index index -> Some (Tui.Style.Indexed index)

  let conv_style (cell : Vterm.ScreenCell.t) : Tui.Style.t =
    {
      fg = conv_color cell.fg;
      bg = conv_color cell.bg;
      add_modifier = conv_mod cell.style;
      sub_modifier = 0;
    }

  let render f (pt : Proc_term.t) (area : Tui.Rect.t) =
    let buf = Buffer.create 4 in
    Tui.Rect.iter
      (fun x y ->
        let cell =
          let x' = x - area.x in
          let y' = y - area.y in
          let { Vterm.rows = h'; cols = w' } = Vterm.getSize pt.vterm in
          if x' >= 0 && x' < w' && y' >= 0 && y' < h' then
            Vterm.Screen.getCell ~row:y' ~col:x' pt.vterm
          else (
            [%log
              warn "Cell is out of bounds: x:%d y:%d w:%d h:%d." x' y' w' h'];
            Vterm.ScreenCell.empty)
        in

        Buffer.clear buf;
        let s =
          try
            Buffer.add_utf_8_uchar buf cell.char;
            Buffer.contents buf
          with ex ->
            [%log
              warn "Error rendering vterm cell (0x%x): %s"
                (Uchar.to_int cell.char) (Printexc.to_string ex)];
            " "
        in
        let style = conv_style cell in
        if cell == Vterm.ScreenCell.empty then ()
        else Tui.render_string f ~style s Tui.Rect.{ x; y; w = 1; h = 1 };

        ())
      area
end

let render f (area : Tui.Rect.t) =
  (let w = max 1 area.w in
   let h = max 1 area.h in
   let w', h' = !State.term_size in
   if w' <> w || h' <> h then Engine.resize_term (w, h));
  let proc = State.get_current () in
  Tui.Rect.iter
    (fun x y -> Tui.render_string f " " Tui.Rect.{ x; y; w = 1; h = 1 })
    area;
  try
    match proc with
    | Some { state = Running kind | Stopping kind; _ } -> (
        match kind with
        | Simple ps -> Simple.render f ps area
        | Vterm pt -> Vterm.render f pt area)
    | _ -> ()
  with ex -> [%log err "Term render error: %s" (Printexc.to_string ex)]
