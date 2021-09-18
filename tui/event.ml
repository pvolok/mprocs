(* Key event *)

type key_code =
  | Backspace
  | Enter
  | Left
  | Right
  | Up
  | Down
  | Home
  | End
  | Page_up
  | Page_down
  | Tab
  | Back_tab
  | Delete
  | Insert
  | F of int
  | Char of int
  | Null
  | Esc
[@@deriving show]

type key_mods = {
  shift : bool;
  control : bool;
  alt : bool;
}
[@@deriving show]

type key_event = {
  code : key_code;
  modifiers : key_mods;
}
[@@deriving show]

(* Mouse event *)

type mouse_button =
  | Left
  | Right
  | Middle
[@@deriving show]

type mouse_event_kind =
  | Down of mouse_button
  | Up of mouse_button
  | Drag of mouse_button
  | Moved
  | Scroll_down
  | Scroll_up
[@@deriving show]

type mouse_event = {
  kind : mouse_event_kind;
  column : int;
  row : int;
  modifiers : key_mods;
}
[@@deriving show]

(* Event *)

type t =
  | Key of key_event
  | Mouse of mouse_event
  | Resize of int * int
[@@deriving show]
