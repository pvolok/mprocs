open Base

(* Key event *)

module Key = struct
  type code =
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
  [@@deriving compare, hash, sexp_of, show]

  type mods = {
    shift : bool;
    control : bool;
    alt : bool;
  }
  [@@deriving compare, hash, sexp_of, show]

  type t = {
    code : code;
    modifiers : mods;
  }
  [@@deriving compare, hash, sexp_of, show]
end

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
  modifiers : Key.mods;
}
[@@deriving show]

(* Event *)

type t =
  | Key of Key.t
  | Mouse of mouse_event
  | Resize of int * int
[@@deriving show]
