open Notty
open Nottui

open Lwd_infix

let conv_color16 =
  let open A in
  function
  | 0 -> black
  | 1 -> red
  | 2 -> green
  | 3 -> yellow
  | 4 -> blue
  | 5 -> magenta
  | 6 -> cyan
  | 7 -> white
  | 8 -> lightblack
  | 9 -> lightred
  | 0xa -> lightgreen
  | 0xb -> lightyellow
  | 0xc -> lightblue
  | 0xd -> lightmagenta
  | 0xe -> lightcyan
  | 0xf -> lightwhite
  | i -> failwith (Printf.sprintf "Bad color index: %d" i)

let apply_fg color attr =
  match Vterm.Color.unpack color with
  | DefaultForeground -> attr
  | DefaultBackground ->
      [%log debug "Applying DefaultBackground to foreground"];
      attr
  | Rgb (r, g, b) -> A.(attr ++ fg (rgb_888 ~r ~g ~b))
  | Index i -> A.(attr ++ fg (conv_color16 i))

let apply_bg color attr =
  match Vterm.Color.unpack color with
  | DefaultForeground ->
      [%log debug "Applying DefaultForeground to background"];
      attr
  | DefaultBackground -> attr
  | Rgb (r, g, b) -> A.(attr ++ bg (rgb_888 ~r ~g ~b))
  | Index i -> A.(attr ++ bg (conv_color16 i))

let apply_style (style : Vterm.Style.t) attr =
  let open A in
  let attr = if Vterm.Style.isBold style then attr ++ st bold else attr in
  let attr = if Vterm.Style.isItalic style then attr ++ st italic else attr in
  let attr =
    if Vterm.Style.isUnderline style then attr ++ st underline else attr
  in
  attr

let cell_to_image (cell : Vterm.ScreenCell.t) =
  let attr =
    A.empty |> apply_fg cell.fg |> apply_bg cell.bg |> apply_style cell.style
  in

  let code = Uchar.to_int cell.char in
  let ui =
    if code = 0 then I.char ~attr ' ' 1 1 else I.uchar ~attr cell.char 1 1
  in
  ui

let render_term vt (w, h) =
  let ret =
    I.tabulate w h (fun x y ->
        try
          let cell = Vterm.Screen.getCell ~row:y ~col:x vt in
          cell_to_image cell
        with ex -> I.char ' ' 1 1)
  in
  ret

let render_simple ps (w, h) =
  let lines = Proc_simple.peek_lines ps h in
  let lines =
    List.map
      (fun line ->
        let line =
          String.to_seq line
          |> Seq.map (fun c ->
                 match c with '\t' -> " " | _ -> Printf.sprintf "%c" c)
          |> List.of_seq |> String.concat ""
        in
        let line =
          if String.length line > w then String.sub line 0 w else line
        in
        let img = line |> I.strf ~w "%s" in
        img)
      lines
  in
  I.vcat lines

let make ~on_resize size' =
  let tick_var = Lwd.var 0 in
  let tick' = Lwd.get tick_var in

  let scheduled = ref false in
  let schedule () =
    if not !scheduled then (
      scheduled := true;
      Lwt.on_success (Lwt.pause ()) (fun () ->
          scheduled := false;
          tick_var $= Lwd.peek tick_var + 1))
  in

  let cur_dispose = ref Dispose.empty in
  let kind' =
    Lwd.map State.kind' ~f:(fun kind ->
        Dispose.dispose !cur_dispose;
        cur_dispose := Dispose.empty;

        (match kind with
        | Some kind -> (
            match kind with
            | Simple ps ->
                let dispose = Dispose.empty in

                let dispose =
                  Listeners.addl ps.Proc_simple.on_update schedule dispose
                in

                cur_dispose := dispose
            | Vterm pt ->
                let dispose = Dispose.empty in

                let dispose =
                  Listeners.addl pt.Proc_term.on_damage
                    (fun _rect -> schedule ())
                    dispose
                in

                cur_dispose := dispose)
        | None -> ());
        kind)
  in

  let last_size = ref (0, 0) in
  let on_resize w h =
    let w0, h0 = !last_size in
    if w0 <> w || h0 <> h then (
      last_size := (w, h);
      on_resize ~w ~h)
  in

  let$ w, h = size' and$ kind = kind' and$ tick = tick' in
  [%log debug "Term frame %d (%dx%d)" tick w h];

  on_resize w h;

  (try
     match kind with
     | Some kind -> (
         match kind with
         | Simple ps -> render_simple ps (w, h)
         | Vterm pt -> render_term pt.Proc_term.vterm (w, h))
     | None -> I.void w h
   with ex ->
     let error = Printexc.to_string ex in
     [%log err "%s" error];
     I.string error)
  |> Ui.atom
