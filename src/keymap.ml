type t =
  | Quit
  | Select_next
  | Select_prev
  | Kill_proc
  | Start_proc
  | Focus_term
  | Focus_procs

let procs = Hashtbl.create 8
let term = Hashtbl.create 8

module Ev = Tui.Event

let bind map ?(ctrl = false) ?(shift = false) ?(alt = false) code act =
  let mods = { Ev.control = ctrl; shift; alt } in
  Hashtbl.replace map { Ev.code; modifiers = mods } act

let bind_c map ?ctrl ?shift ?alt c =
  bind map ?ctrl ?shift ?alt (Ev.Char (Char.code c))

let () =
  bind_c procs 'q' Quit;
  bind_c procs 'j' Select_next;
  bind_c procs 'k' Select_prev;
  bind_c procs 'x' Kill_proc;
  bind_c procs 's' Start_proc;
  bind_c procs ~ctrl:true 'a' Focus_term;

  bind_c term ~ctrl:true 'a' Focus_procs

let handle map key = Hashtbl.find_opt map key

(***************)

let to_string (key : Tui.Event.key_event) =
  let buf = Buffer.create 8 in

  if key.modifiers.control then Buffer.add_string buf "C-";
  if key.modifiers.shift then Buffer.add_string buf "S-";
  if key.modifiers.alt then Buffer.add_string buf "M-";

  let add_s = Buffer.add_string buf in
  (match key.code with
  | Char code -> Buffer.add_utf_8_uchar buf (Uchar.of_int code)
  | Tab -> add_s "Tab"
  | Down -> add_s "Down"
  | Up -> add_s "Up"
  | Left -> add_s "Left"
  | Right -> add_s "Right"
  | Backspace -> add_s "Bksp"
  | Delete -> add_s "Del"
  | Enter -> add_s "Enter"
  | Esc -> add_s "Esc"
  | F x -> add_s (Printf.sprintf "F%d" x)
  | Page_up -> add_s "PgUp"
  | Page_down -> add_s "PgDn"
  | Home -> add_s "Home"
  | End -> add_s "End"
  | Insert -> add_s "Ins"
  | Back_tab -> add_s "BackTab"
  | Null -> add_s "Null");

  Buffer.contents buf
