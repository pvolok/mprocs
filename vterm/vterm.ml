type terminal

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

module Rect = struct
  type t = {
    startRow : int;
    endRow : int;
    startCol : int;
    endCol : int;
  }

  let toString { startRow; endRow; startCol; endCol } =
    Printf.sprintf "startRow: %d startCol: %d endRow: %d endCol: %d" startRow
      startCol endRow endCol
end

module Pos = struct
  type t = {
    row : int;
    col : int;
  }

  let toString { row; col } = Printf.sprintf "row: %d col: %d" row col
end

module TermProp = struct
  module CursorShape = struct
    type t =
      | Block
      | Underline
      | BarLeft
      | Unknown

    let toString = function
      | Block -> "Block"
      | Underline -> "Underline"
      | BarLeft -> "BarLeft"
      | Unknown -> "Unknown"
  end

  module Mouse = struct
    type t =
      | None
      | Click
      | Drag
      | Move

    let toString = function
      | None -> "None"
      | Click -> "Click"
      | Drag -> "Drag"
      | Move -> "Move"
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

  let toString = function
    | None -> "None"
    | CursorVisible viz -> Printf.sprintf "CursorVisible: %b" viz
    | CursorBlink blink -> Printf.sprintf "CursorBlink: %B" blink
    | AltScreen alt -> Printf.sprintf "AltScreen: %b" alt
    | Title str -> Printf.sprintf "Title: %s" str
    | IconName str -> Printf.sprintf "IconName: %s" str
    | Reverse rev -> Printf.sprintf "Reverse: %b" rev
    | CursorShape cursor ->
        Printf.sprintf "CursorShape: %s" (CursorShape.toString cursor)
    | Mouse mouse -> Printf.sprintf "Mouse: %s" (Mouse.toString mouse)
end

module Color = struct
  type raw = int

  type t =
    | DefaultForeground
    | DefaultBackground
    | Rgb of int * int * int
    | Index of int

  let defaultForeground = 1024
  let defaultBackground = 1025

  let toString = function
    | DefaultForeground -> "DefaultForeground"
    | DefaultBackground -> "DefaultBackground"
    | Rgb (r, g, b) -> Printf.sprintf "rgb(%d, %d, %d)" r g b
    | Index idx -> Printf.sprintf "index(%d)" idx

  let unpack raw =
    let controlBit = raw land 3 in
    match controlBit with
    | 0 -> DefaultBackground
    | 1 -> DefaultForeground
    | 2 ->
        let r = (raw land (255 lsl 18)) lsr 18 in
        let g = (raw land (255 lsl 10)) lsr 10 in
        let b = (raw land (255 lsl 2)) lsr 2 in
        (Rgb (r, g, b) [@explicit_arity])
    | 3 ->
        let idx = (raw land (255 lsl 2)) lsr 2 in
        (Index idx [@explicit_arity])
    | _ -> DefaultForeground
end

module Style = struct
  type t = int
  let isBold v = v land 1 = 1
  let isBold v = let a () = v land 1 = 1 in false
  let isItalic v = v land 2 = 2
  let isUnderline v = v land 4 = 4
end

module ScreenCell = struct
  type t = {
    char : Uchar.t;
    fg : Color.raw;
    bg : Color.raw;
    style : Style.t;
  }

  let empty : t =
    {
      char = Uchar.of_int 0;
      fg = Color.defaultForeground;
      bg = Color.defaultBackground;
      style = 0;
    }
end

type sb_line

type callbacks = {
  onTermOutput : (string -> unit) ref;
  onScreenDamage : (Rect.t -> unit) ref;
  onScreenMoveRect : (Rect.t -> Rect.t -> unit) ref;
  onScreenMoveCursor : (Pos.t -> Pos.t -> bool -> unit) ref;
  onScreenSetTermProp : (TermProp.t -> unit) ref;
  onScreenBell : (unit -> unit) ref;
  onScreenResize : (size -> unit) ref;
  onScreenScrollbackPushLine : (sb_line -> unit) ref;
  onScreenScrollbackPopLine : (unit -> sb_line option) ref;
}

type t = {
  uniqueId : int;
  terminal : terminal;
  callbacks : callbacks;
}

let idToOutputCallback : (int, callbacks) Hashtbl.t = Hashtbl.create 8

module Internal = struct
  let uniqueId = ref 0
  external newVterm : int -> int -> int -> terminal
    = "reason_libvterm_vterm_new"
  external freeVterm : terminal -> unit = "reason_libvterm_vterm_free"
  external set_utf8 : terminal -> bool -> unit
    = "reason_libvterm_vterm_set_utf8"
  external get_utf8 : terminal -> bool = "reason_libvterm_vterm_get_utf8"
  external get_size : terminal -> size = "reason_libvterm_vterm_get_size"
  external set_size : terminal -> size -> unit
    = "reason_libvterm_vterm_set_size"
  external input_write : terminal -> string -> int
    = "reason_libvterm_vterm_input_write"
  external keyboard_unichar : terminal -> Int32.t -> modifier -> unit
    = "reason_libvterm_vterm_keyboard_unichar"
  external keyboard_key : terminal -> key -> modifier -> unit
    = "reason_libvterm_vterm_keyboard_key"
  external screen_get_cell : terminal -> int -> int -> ScreenCell.t
    = "reason_libvterm_vterm_screen_get_cell"
  external screen_enable_altscreen : terminal -> int -> unit
    = "reason_libvterm_vterm_screen_enable_altscreen"

  let onOutput (id : int) (output : string) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onTermOutput; _ } -> !onTermOutput output
    | None -> ()

  let onScreenBell (id : int) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenBell; _ } -> !onScreenBell ()
    | None -> ()

  let onScreenResize (id : int) (rows : int) (cols : int) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenResize; _ } -> !onScreenResize { rows; cols }
    | None -> ()

  let onScreenDamage (id : int) (rect : Rect.t) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenDamage; _ } -> !onScreenDamage rect
    | None -> ()

  let onScreenMoveCursor (id : int) newRow newCol oldRow oldCol visible =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenMoveCursor; _ } ->
        !onScreenMoveCursor
          (let open Pos in
          { row = newRow; col = newCol })
          (let open Pos in
          { row = oldRow; col = oldCol })
          visible
    | None -> ()

  let onScreenMoveRect (id : int) (destStartRow : int) (destStartCol : int)
      (destEndRow : int) (destEndCol : int) (srcStartRow : int)
      (srcStartCol : int) (srcEndRow : int) (srcEndCol : int) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenMoveRect; _ } ->
        !onScreenMoveRect
          Rect.
            {
              startRow = destStartRow;
              startCol = destStartCol;
              endRow = destEndRow;
              endCol = destEndCol;
            }
          Rect.
            {
              startRow = srcStartRow;
              startCol = srcStartCol;
              endRow = srcEndRow;
              endCol = srcEndCol;
            }
    | None -> ()

  let onScreenSetTermProp (id : int) (termProp : TermProp.t) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenSetTermProp; _ } -> !onScreenSetTermProp termProp
    | None -> ()

  let onScreenSbPushLine (id : int) (cells : sb_line) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenScrollbackPushLine; _ } ->
        !onScreenScrollbackPushLine cells
    | None -> ()

  let onScreenSbPopLine (id : int) =
    match Hashtbl.find_opt idToOutputCallback id with
    | Some { onScreenScrollbackPopLine; _ } -> !onScreenScrollbackPopLine ()
    | None -> None

  let _ = Callback.register "reason_libvterm_onOutput" onOutput
  let _ = Callback.register "reason_libvterm_onScreenBell" onScreenBell
  let _ = Callback.register "reason_libvterm_onScreenResize" onScreenResize
  let _ = Callback.register "reason_libvterm_onScreenDamage" onScreenDamage
  let _ =
    Callback.register "reason_libvterm_onScreenMoveCursor" onScreenMoveCursor
  let _ = Callback.register "reason_libvterm_onScreenMoveRect" onScreenMoveRect
  let _ =
    Callback.register "reason_libvterm_onScreenSetTermProp" onScreenSetTermProp
  let _ =
    Callback.register "reason_libvterm_onScreenSbPushLine" onScreenSbPushLine
  let _ =
    Callback.register "reason_libvterm_onScreenSbPopLine" onScreenSbPopLine
end

module Screen = struct
  let setBellCallback ~onBell terminal =
    terminal.callbacks.onScreenBell := onBell

  let setResizeCallback ~onResize terminal =
    terminal.callbacks.onScreenResize := onResize

  let setDamageCallback ~onDamage terminal =
    terminal.callbacks.onScreenDamage := onDamage

  let setMoveCursorCallback ~onMoveCursor terminal =
    terminal.callbacks.onScreenMoveCursor := onMoveCursor

  let setMoveRectCallback ~onMoveRect terminal =
    terminal.callbacks.onScreenMoveRect := onMoveRect

  let setScrollbackPopCallback ~onPopLine terminal =
    terminal.callbacks.onScreenScrollbackPopLine := onPopLine

  let setScrollbackPushCallback ~onPushLine terminal =
    terminal.callbacks.onScreenScrollbackPushLine := onPushLine

  let getCell ~row ~col { terminal; _ } =
    Internal.screen_get_cell terminal row col

  let setAltScreen ~enabled { terminal; _ } =
    Internal.screen_enable_altscreen terminal
      (match enabled with true -> 1 | false -> 0)

  let setTermPropCallback ~onSetTermProp terminal =
    terminal.callbacks.onScreenSetTermProp := onSetTermProp
end

module Keyboard = struct
  let input { terminal; _ } key (mods : modifier) =
    match key with
    | Unicode uchar ->
        let key = uchar |> Uchar.to_int |> Int32.of_int in
        Internal.keyboard_unichar terminal key mods
    | key -> Internal.keyboard_key terminal key mods
end

let make ~rows ~cols =
  incr Internal.uniqueId;
  let uniqueId = !Internal.uniqueId in
  let terminal = Internal.newVterm uniqueId rows cols in
  let onTermOutput = ref (fun _ -> ()) in
  let onScreenDamage = ref (fun _ -> ()) in
  let onScreenMoveRect = ref (fun _ _ -> ()) in
  let onScreenMoveCursor = ref (fun _ _ _ -> ()) in
  let onScreenBell = ref (fun () -> ()) in
  let onScreenResize = ref (fun _ -> ()) in
  let onScreenSetTermProp = ref (fun _ -> ()) in
  let onScreenScrollbackPushLine = ref (fun _ -> ()) in
  let onScreenScrollbackPopLine = ref (fun _ : sb_line option -> None) in
  let callbacks =
    {
      onTermOutput;
      onScreenDamage;
      onScreenMoveRect;
      onScreenMoveCursor;
      onScreenSetTermProp;
      onScreenBell;
      onScreenResize;
      onScreenScrollbackPushLine;
      onScreenScrollbackPopLine;
    }
  in
  let wrappedTerminal : t = { terminal; uniqueId; callbacks } in
  Hashtbl.add idToOutputCallback uniqueId callbacks;
  let () =
    Gc.finalise
      (fun ({ terminal; uniqueId; _ } : t) ->
        Internal.freeVterm terminal;
        Hashtbl.remove idToOutputCallback uniqueId)
      wrappedTerminal
  in
  wrappedTerminal

let setOutputCallback ~onOutput terminal =
  terminal.callbacks.onTermOutput := onOutput

let setUtf8 ~utf8 { terminal; _ } = Internal.set_utf8 terminal utf8

let getUtf8 { terminal; _ } = Internal.get_utf8 terminal

let setSize ~size { terminal; _ } = Internal.set_size terminal size

let getSize { terminal; _ } = Internal.get_size terminal

let write ~input { terminal; _ } = Internal.input_write terminal input
