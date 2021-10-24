open Base

type t =
  | Quit
  | Select_next
  | Select_prev
  | Kill_proc
  | Start_proc
  | Focus_term
  | Focus_procs
[@@deriving show]

let procs = Hashtbl.Poly.create ()
let term = Hashtbl.Poly.create ()

module Ev = Tui.Event
module KeyCode = Tui.Event.KeyCode
module Key = Tui.Event.KeyEvent
module KeyMods = Tui.Event.KeyMods

let bind map ?(ctrl = false) ?(shift = false) ?(alt = false) code act =
  let mods =
    {
      KeyMods.control = (if ctrl then 1 else 0);
      shift = (if shift then 1 else 0);
      alt = (if alt then 1 else 0);
    }
  in
  Hashtbl.set map ~key:{ Key.code; modifiers = mods } ~data:act

let bind_c map ?ctrl ?shift ?alt c =
  bind map ?ctrl ?shift ?alt (KeyCode.Char (Char.to_int c))

let () =
  bind_c procs 'q' Quit;
  bind_c procs 'j' Select_next;
  bind_c procs 'k' Select_prev;
  bind_c procs 'x' Kill_proc;
  bind_c procs 's' Start_proc;
  bind_c procs ~ctrl:true 'a' Focus_term;

  bind_c term ~ctrl:true 'a' Focus_procs

let handle map key = Hashtbl.find map key

(***************)

let to_string (key : Tui.Event.KeyEvent.t) =
  let buf = Buffer.create 8 in

  if key.modifiers.control <> 0 then Buffer.add_string buf "C-";
  if key.modifiers.shift <> 0 then Buffer.add_string buf "S-";
  if key.modifiers.alt <> 0 then Buffer.add_string buf "M-";

  let add_s = Buffer.add_string buf in
  (match key.code with
  | Char code -> Caml.Buffer.add_utf_8_uchar buf (Uchar.of_scalar_exn code)
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
  | PageUp -> add_s "PgUp"
  | PageDown -> add_s "PgDn"
  | Home -> add_s "Home"
  | End -> add_s "End"
  | Insert -> add_s "Ins"
  | BackTab -> add_s "BackTab"
  | Null -> add_s "Null");

  Buffer.contents buf
