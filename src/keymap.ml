let key ?(control = false) ?(meta = false) ?(shift = false) c : Nottui.Ui.key =
  let mods = if control then [ `Ctrl ] else [] in
  let mods = if meta then `Meta :: mods else mods in
  let mods = if shift then `Shift :: mods else mods in
  (`ASCII c, mods)

let to_string (key : Nottui.Ui.key) =
  let main, mods = key in
  let buf = Buffer.create 8 in

  List.iter
    (function
      | `Ctrl -> Buffer.add_string buf "C-"
      | `Meta -> Buffer.add_string buf "M-"
      | `Shift -> Buffer.add_string buf "S-")
    mods;

  let add_s = Buffer.add_string buf in
  (match main with
  | `ASCII c -> Buffer.add_char buf c
  | `Uchar uc -> Buffer.add_utf_8_uchar buf uc
  | `Tab -> add_s "Tab"
  | `Arrow `Down -> add_s "Down"
  | `Arrow `Up -> add_s "Up"
  | `Arrow `Left -> add_s "Left"
  | `Arrow `Right -> add_s "Right"
  | `Backspace -> add_s "Bksp"
  | `Delete -> add_s "Del"
  | `Enter -> add_s "Enter"
  | `Escape -> add_s "Esc"
  | `Function x -> add_s (Printf.sprintf "F%d" x)
  | `Page `Up -> add_s "PgUp"
  | `Page `Down -> add_s "PgDn"
  | `Home -> add_s "Home"
  | `End -> add_s "End"
  | `Insert -> add_s "Ins"
  | `Copy -> add_s "Copy"
  | `Paste -> add_s "Paste"
  | `Focus _ -> add_s "Focus?");

  Buffer.contents buf

let procs_help =
  [
    ("Quit", key 'q');
    ("Output", key ~control:true 'a');
    ("Kill", key 'x');
    ("Start", key 's');
    ("Up", key 'k');
    ("Down", key 'j');
  ]

let output_help = [ ("Process list", key ~control:true 'a') ]
