type t

type modifier =
  | None
  | Shift
  | Alt
  | Control
  | All

type key =
  | Unicode of Uchar.t
  | Enter
  | Tab
  | Backspace
  | Escape
  | Up
  | Down
  | Left
  | Right
  | Insert
  | Delete
  | Home
  | End
  | PageUp
  | PageDown

type size = {
  rows : int;
  cols : int;
}

val make : rows:int -> cols:int -> t
val setOutputCallback : onOutput:(string -> unit) -> t -> unit
val setUtf8 : utf8:bool -> t -> unit
val getUtf8 : t -> bool
val setSize : size:size -> t -> unit
val getSize : t -> size
val write : input:string -> t -> int

module Rect : sig
  type t = {
    startRow : int;
    endRow : int;
    startCol : int;
    endCol : int;
  }

  val toString : t -> string
end

module Pos : sig
  type t = {
    row : int;
    col : int;
  }

  val toString : t -> string
end

module TermProp : sig
  module CursorShape : sig
    type t =
      | Block
      | Underline
      | BarLeft
      | Unknown
    val toString : t -> string
  end

  module Mouse : sig
    type t =
      | None
      | Click
      | Drag
      | Move
    val toString : t -> string
  end

  type t =
    | None
    | CursorVisible of bool
    | CursorBlink of bool
    | AltScreen of bool
    | Title of string
    | IconName of string
    | Reverse of bool
    | CursorShape of CursorShape.t
    | Mouse of Mouse.t

  val toString : t -> string
end

module Color : sig
  type raw

  type t =
    | DefaultForeground
    | DefaultBackground
    | Rgb of int * int * int
    | Index of int

  val unpack : raw -> t
  val toString : t -> string
end

module Style : sig
  type t
  val isBold : t -> bool
  val isUnderline : t -> bool
  val isItalic : t -> bool
end

module ScreenCell : sig
  type t = {
    char : Uchar.t;
    fg : Color.raw;
    bg : Color.raw;
    style : Style.t;
  }

  val empty : t
end

type sb_line

module Screen : sig
  val setBellCallback : onBell:(unit -> unit) -> t -> unit
  val setResizeCallback : onResize:(size -> unit) -> t -> unit
  val setDamageCallback : onDamage:(Rect.t -> unit) -> t -> unit
  val setMoveCursorCallback :
    onMoveCursor:(Pos.t -> Pos.t -> bool -> unit) -> t -> unit
  val setMoveRectCallback : onMoveRect:(Rect.t -> Rect.t -> unit) -> t -> unit
  val setTermPropCallback : onSetTermProp:(TermProp.t -> unit) -> t -> unit
  val setScrollbackPopCallback : onPopLine:(unit -> sb_line option) -> t -> unit
  val setScrollbackPushCallback : onPushLine:(sb_line -> unit) -> t -> unit
  val getCell : row:int -> col:int -> t -> ScreenCell.t
  val setAltScreen : enabled:bool -> t -> unit
end

module Keyboard : sig
  val input : t -> key -> modifier -> unit
end
