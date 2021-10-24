open Ctypes

module B = Types_bindings.Make (Types_stubs)

module Rect = struct
  type t = {
    x : int;
    y : int;
    w : int;
    h : int;
  }[@@deriving compare, hash, sexp_of,
              show]
  
  let of_c x =
    {
      x = getf x B.Rect.x |> Unsigned.UInt16.to_int;
      y = getf x B.Rect.y |> Unsigned.UInt16.to_int;
      w = getf x B.Rect.w |> Unsigned.UInt16.to_int;
      h = getf x B.Rect.h |> Unsigned.UInt16.to_int;
    }
  
  
  let to_c x =
    let v = make B.Rect.t in
    setf v B.Rect.x (Unsigned.UInt16.of_int x.x);
    setf v B.Rect.y (Unsigned.UInt16.of_int x.y);
    setf v B.Rect.w (Unsigned.UInt16.of_int x.w);
    setf v B.Rect.h (Unsigned.UInt16.of_int x.h);
    v
    
  
  
end

module Constraint = struct
  type t =
    | Percentage of int
    | Ratio of int * int
    | Length of int
    | Min of int
    | Max of int
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.Constraint.body in
    match getf x B.Constraint.tag with
    | `Percentage ->
      Percentage (getf body B.Constraint.Body.percentage_0 |> Unsigned.UInt16.to_int)
    | `Ratio ->
      Ratio (getf body B.Constraint.Body.ratio_0 |> Unsigned.UInt32.to_int,
             getf body B.Constraint.Body.ratio_1 |> Unsigned.UInt32.to_int)
    | `Length ->
      Length (getf body B.Constraint.Body.length_0 |> Unsigned.UInt16.to_int)
    | `Min ->
      Min (getf body B.Constraint.Body.min_0 |> Unsigned.UInt16.to_int)
    | `Max ->
      Max (getf body B.Constraint.Body.max_0 |> Unsigned.UInt16.to_int)
    
  let to_c x =
    let v = make B.Constraint.t in
    let body = make B.Constraint.Body.t in
    let () = 
      match x with
      | Percentage (a0) ->
        setf v B.Constraint.tag `Percentage;
        setf body B.Constraint.Body.percentage_0 (Unsigned.UInt16.of_int a0);
        ()
      | Ratio (a0, a1) ->
        setf v B.Constraint.tag `Ratio;
        setf body B.Constraint.Body.ratio_0 (Unsigned.UInt32.of_int a0);
        setf body B.Constraint.Body.ratio_1 (Unsigned.UInt32.of_int a1);
        ()
      | Length (a0) ->
        setf v B.Constraint.tag `Length;
        setf body B.Constraint.Body.length_0 (Unsigned.UInt16.of_int a0);
        ()
      | Min (a0) ->
        setf v B.Constraint.tag `Min;
        setf body B.Constraint.Body.min_0 (Unsigned.UInt16.of_int a0);
        ()
      | Max (a0) ->
        setf v B.Constraint.tag `Max;
        setf body B.Constraint.Body.max_0 (Unsigned.UInt16.of_int a0);
        ()
      
    in
    setf v B.Constraint.body body;
    v
    
  module B = B.Constraint
end
module Direction = struct
  type t =
    | Horizontal
    | Vertical
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.Direction.body in
    match getf x B.Direction.tag with
    | `Horizontal ->
      Horizontal
    | `Vertical ->
      Vertical
    
  let to_c x =
    let v = make B.Direction.t in
    let body = make B.Direction.Body.t in
    let () = 
      match x with
      | Horizontal ->
        setf v B.Direction.tag `Horizontal;
        ()
      | Vertical ->
        setf v B.Direction.tag `Vertical;
        ()
      
    in
    setf v B.Direction.body body;
    v
    
  module B = B.Direction
end
module Color = struct
  type t =
    | Reset
    | Black
    | Red
    | Green
    | Yellow
    | Blue
    | Magenta
    | Cyan
    | Gray
    | DarkGray
    | LightRed
    | LightGreen
    | LightYellow
    | LightBlue
    | LightMagenta
    | LightCyan
    | White
    | Rgb of int * int * int
    | Indexed of int
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.Color.body in
    match getf x B.Color.tag with
    | `Reset ->
      Reset
    | `Black ->
      Black
    | `Red ->
      Red
    | `Green ->
      Green
    | `Yellow ->
      Yellow
    | `Blue ->
      Blue
    | `Magenta ->
      Magenta
    | `Cyan ->
      Cyan
    | `Gray ->
      Gray
    | `DarkGray ->
      DarkGray
    | `LightRed ->
      LightRed
    | `LightGreen ->
      LightGreen
    | `LightYellow ->
      LightYellow
    | `LightBlue ->
      LightBlue
    | `LightMagenta ->
      LightMagenta
    | `LightCyan ->
      LightCyan
    | `White ->
      White
    | `Rgb ->
      Rgb (getf body B.Color.Body.rgb_0 |> Unsigned.UInt8.to_int,
           getf body B.Color.Body.rgb_1 |> Unsigned.UInt8.to_int,
           getf body B.Color.Body.rgb_2 |> Unsigned.UInt8.to_int)
    | `Indexed ->
      Indexed (getf body B.Color.Body.indexed_0 |> Unsigned.UInt8.to_int)
    
  let to_c x =
    let v = make B.Color.t in
    let body = make B.Color.Body.t in
    let () = 
      match x with
      | Reset ->
        setf v B.Color.tag `Reset;
        ()
      | Black ->
        setf v B.Color.tag `Black;
        ()
      | Red ->
        setf v B.Color.tag `Red;
        ()
      | Green ->
        setf v B.Color.tag `Green;
        ()
      | Yellow ->
        setf v B.Color.tag `Yellow;
        ()
      | Blue ->
        setf v B.Color.tag `Blue;
        ()
      | Magenta ->
        setf v B.Color.tag `Magenta;
        ()
      | Cyan ->
        setf v B.Color.tag `Cyan;
        ()
      | Gray ->
        setf v B.Color.tag `Gray;
        ()
      | DarkGray ->
        setf v B.Color.tag `DarkGray;
        ()
      | LightRed ->
        setf v B.Color.tag `LightRed;
        ()
      | LightGreen ->
        setf v B.Color.tag `LightGreen;
        ()
      | LightYellow ->
        setf v B.Color.tag `LightYellow;
        ()
      | LightBlue ->
        setf v B.Color.tag `LightBlue;
        ()
      | LightMagenta ->
        setf v B.Color.tag `LightMagenta;
        ()
      | LightCyan ->
        setf v B.Color.tag `LightCyan;
        ()
      | White ->
        setf v B.Color.tag `White;
        ()
      | Rgb (a0, a1, a2) ->
        setf v B.Color.tag `Rgb;
        setf body B.Color.Body.rgb_0 (Unsigned.UInt8.of_int a0);
        setf body B.Color.Body.rgb_1 (Unsigned.UInt8.of_int a1);
        setf body B.Color.Body.rgb_2 (Unsigned.UInt8.of_int a2);
        ()
      | Indexed (a0) ->
        setf v B.Color.tag `Indexed;
        setf body B.Color.Body.indexed_0 (Unsigned.UInt8.of_int a0);
        ()
      
    in
    setf v B.Color.body body;
    v
    
  module B = B.Color
end
module ColorOpt = struct
  type t =
    | Some of Color.t
    | None
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.ColorOpt.body in
    match getf x B.ColorOpt.tag with
    | `Some ->
      Some (getf body B.ColorOpt.Body.some_0 |> Color.of_c)
    | `None ->
      None
    
  let to_c x =
    let v = make B.ColorOpt.t in
    let body = make B.ColorOpt.Body.t in
    let () = 
      match x with
      | Some (a0) ->
        setf v B.ColorOpt.tag `Some;
        setf body B.ColorOpt.Body.some_0 (Color.to_c a0);
        ()
      | None ->
        setf v B.ColorOpt.tag `None;
        ()
      
    in
    setf v B.ColorOpt.body body;
    v
    
  module B = B.ColorOpt
end
module Style = struct
  type t = {
    fg : ColorOpt.t;
    bg : ColorOpt.t;
    add_modifier : int;
    sub_modifier : int;
  }[@@deriving compare, hash, sexp_of,
              show]
  
  let of_c x =
    {
      fg = getf x B.Style.fg |> ColorOpt.of_c;
      bg = getf x B.Style.bg |> ColorOpt.of_c;
      add_modifier = getf x B.Style.add_modifier |> Unsigned.UInt16.to_int;
      sub_modifier = getf x B.Style.sub_modifier |> Unsigned.UInt16.to_int;
    }
  
  
  let to_c x =
    let v = make B.Style.t in
    setf v B.Style.fg (ColorOpt.to_c x.fg);
    setf v B.Style.bg (ColorOpt.to_c x.bg);
    setf v B.Style.add_modifier (Unsigned.UInt16.of_int x.add_modifier);
    setf v B.Style.sub_modifier (Unsigned.UInt16.of_int x.sub_modifier);
    v
    
  
  
end

module KeyCode = struct
  type t =
    | Backspace
    | Enter
    | Left
    | Right
    | Up
    | Down
    | Home
    | End
    | PageUp
    | PageDown
    | Tab
    | BackTab
    | Delete
    | Insert
    | F of int
    | Char of int
    | Null
    | Esc
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.KeyCode.body in
    match getf x B.KeyCode.tag with
    | `Backspace ->
      Backspace
    | `Enter ->
      Enter
    | `Left ->
      Left
    | `Right ->
      Right
    | `Up ->
      Up
    | `Down ->
      Down
    | `Home ->
      Home
    | `End ->
      End
    | `PageUp ->
      PageUp
    | `PageDown ->
      PageDown
    | `Tab ->
      Tab
    | `BackTab ->
      BackTab
    | `Delete ->
      Delete
    | `Insert ->
      Insert
    | `F ->
      F (getf body B.KeyCode.Body.f_0 |> Unsigned.UInt8.to_int)
    | `Char ->
      Char (getf body B.KeyCode.Body.char_0 |> Unsigned.UInt32.to_int)
    | `Null ->
      Null
    | `Esc ->
      Esc
    
  let to_c x =
    let v = make B.KeyCode.t in
    let body = make B.KeyCode.Body.t in
    let () = 
      match x with
      | Backspace ->
        setf v B.KeyCode.tag `Backspace;
        ()
      | Enter ->
        setf v B.KeyCode.tag `Enter;
        ()
      | Left ->
        setf v B.KeyCode.tag `Left;
        ()
      | Right ->
        setf v B.KeyCode.tag `Right;
        ()
      | Up ->
        setf v B.KeyCode.tag `Up;
        ()
      | Down ->
        setf v B.KeyCode.tag `Down;
        ()
      | Home ->
        setf v B.KeyCode.tag `Home;
        ()
      | End ->
        setf v B.KeyCode.tag `End;
        ()
      | PageUp ->
        setf v B.KeyCode.tag `PageUp;
        ()
      | PageDown ->
        setf v B.KeyCode.tag `PageDown;
        ()
      | Tab ->
        setf v B.KeyCode.tag `Tab;
        ()
      | BackTab ->
        setf v B.KeyCode.tag `BackTab;
        ()
      | Delete ->
        setf v B.KeyCode.tag `Delete;
        ()
      | Insert ->
        setf v B.KeyCode.tag `Insert;
        ()
      | F (a0) ->
        setf v B.KeyCode.tag `F;
        setf body B.KeyCode.Body.f_0 (Unsigned.UInt8.of_int a0);
        ()
      | Char (a0) ->
        setf v B.KeyCode.tag `Char;
        setf body B.KeyCode.Body.char_0 (Unsigned.UInt32.of_int a0);
        ()
      | Null ->
        setf v B.KeyCode.tag `Null;
        ()
      | Esc ->
        setf v B.KeyCode.tag `Esc;
        ()
      
    in
    setf v B.KeyCode.body body;
    v
    
  module B = B.KeyCode
end
module KeyMods = struct
  type t = {
    shift : int;
    control : int;
    alt : int;
  }[@@deriving compare, hash, sexp_of,
              show]
  
  let of_c x =
    {
      shift = getf x B.KeyMods.shift |> Unsigned.UInt8.to_int;
      control = getf x B.KeyMods.control |> Unsigned.UInt8.to_int;
      alt = getf x B.KeyMods.alt |> Unsigned.UInt8.to_int;
    }
  
  
  let to_c x =
    let v = make B.KeyMods.t in
    setf v B.KeyMods.shift (Unsigned.UInt8.of_int x.shift);
    setf v B.KeyMods.control (Unsigned.UInt8.of_int x.control);
    setf v B.KeyMods.alt (Unsigned.UInt8.of_int x.alt);
    v
    
  
  
end

module KeyEvent = struct
  type t = {
    code : KeyCode.t;
    modifiers : KeyMods.t;
  }[@@deriving compare, hash, sexp_of,
              show]
  
  let of_c x =
    {
      code = getf x B.KeyEvent.code |> KeyCode.of_c;
      modifiers = getf x B.KeyEvent.modifiers |> KeyMods.of_c;
    }
  
  
  let to_c x =
    let v = make B.KeyEvent.t in
    setf v B.KeyEvent.code (KeyCode.to_c x.code);
    setf v B.KeyEvent.modifiers (KeyMods.to_c x.modifiers);
    v
    
  
  
end

module MouseButton = struct
  type t =
    | Left
    | Right
    | Middle
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.MouseButton.body in
    match getf x B.MouseButton.tag with
    | `Left ->
      Left
    | `Right ->
      Right
    | `Middle ->
      Middle
    
  let to_c x =
    let v = make B.MouseButton.t in
    let body = make B.MouseButton.Body.t in
    let () = 
      match x with
      | Left ->
        setf v B.MouseButton.tag `Left;
        ()
      | Right ->
        setf v B.MouseButton.tag `Right;
        ()
      | Middle ->
        setf v B.MouseButton.tag `Middle;
        ()
      
    in
    setf v B.MouseButton.body body;
    v
    
  module B = B.MouseButton
end
module MouseEventKind = struct
  type t =
    | Down of MouseButton.t
    | Up of MouseButton.t
    | Drag of MouseButton.t
    | Moved
    | ScrollDown
    | ScrollUp
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.MouseEventKind.body in
    match getf x B.MouseEventKind.tag with
    | `Down ->
      Down (getf body B.MouseEventKind.Body.down_0 |> MouseButton.of_c)
    | `Up ->
      Up (getf body B.MouseEventKind.Body.up_0 |> MouseButton.of_c)
    | `Drag ->
      Drag (getf body B.MouseEventKind.Body.drag_0 |> MouseButton.of_c)
    | `Moved ->
      Moved
    | `ScrollDown ->
      ScrollDown
    | `ScrollUp ->
      ScrollUp
    
  let to_c x =
    let v = make B.MouseEventKind.t in
    let body = make B.MouseEventKind.Body.t in
    let () = 
      match x with
      | Down (a0) ->
        setf v B.MouseEventKind.tag `Down;
        setf body B.MouseEventKind.Body.down_0 (MouseButton.to_c a0);
        ()
      | Up (a0) ->
        setf v B.MouseEventKind.tag `Up;
        setf body B.MouseEventKind.Body.up_0 (MouseButton.to_c a0);
        ()
      | Drag (a0) ->
        setf v B.MouseEventKind.tag `Drag;
        setf body B.MouseEventKind.Body.drag_0 (MouseButton.to_c a0);
        ()
      | Moved ->
        setf v B.MouseEventKind.tag `Moved;
        ()
      | ScrollDown ->
        setf v B.MouseEventKind.tag `ScrollDown;
        ()
      | ScrollUp ->
        setf v B.MouseEventKind.tag `ScrollUp;
        ()
      
    in
    setf v B.MouseEventKind.body body;
    v
    
  module B = B.MouseEventKind
end
module MouseEvent = struct
  type t = {
    kind : MouseEventKind.t;
    column : int;
    row : int;
    modifiers : KeyMods.t;
  }[@@deriving compare, hash, sexp_of,
              show]
  
  let of_c x =
    {
      kind = getf x B.MouseEvent.kind |> MouseEventKind.of_c;
      column = getf x B.MouseEvent.column |> Unsigned.UInt16.to_int;
      row = getf x B.MouseEvent.row |> Unsigned.UInt16.to_int;
      modifiers = getf x B.MouseEvent.modifiers |> KeyMods.of_c;
    }
  
  
  let to_c x =
    let v = make B.MouseEvent.t in
    setf v B.MouseEvent.kind (MouseEventKind.to_c x.kind);
    setf v B.MouseEvent.column (Unsigned.UInt16.of_int x.column);
    setf v B.MouseEvent.row (Unsigned.UInt16.of_int x.row);
    setf v B.MouseEvent.modifiers (KeyMods.to_c x.modifiers);
    v
    
  
  
end

module Event = struct
  type t =
    | Key of KeyEvent.t
    | Mouse of MouseEvent.t
    | Resize of int * int
    | Finished
  [@@deriving compare, hash, sexp_of,
              show]
  let of_c x =
    let body = getf x B.Event.body in
    match getf x B.Event.tag with
    | `Key ->
      Key (getf body B.Event.Body.key_0 |> KeyEvent.of_c)
    | `Mouse ->
      Mouse (getf body B.Event.Body.mouse_0 |> MouseEvent.of_c)
    | `Resize ->
      Resize (getf body B.Event.Body.resize_0 |> Unsigned.UInt16.to_int,
              getf body B.Event.Body.resize_1 |> Unsigned.UInt16.to_int)
    | `Finished ->
      Finished
    
  let to_c x =
    let v = make B.Event.t in
    let body = make B.Event.Body.t in
    let () = 
      match x with
      | Key (a0) ->
        setf v B.Event.tag `Key;
        setf body B.Event.Body.key_0 (KeyEvent.to_c a0);
        ()
      | Mouse (a0) ->
        setf v B.Event.tag `Mouse;
        setf body B.Event.Body.mouse_0 (MouseEvent.to_c a0);
        ()
      | Resize (a0, a1) ->
        setf v B.Event.tag `Resize;
        setf body B.Event.Body.resize_0 (Unsigned.UInt16.of_int a0);
        setf body B.Event.Body.resize_1 (Unsigned.UInt16.of_int a1);
        ()
      | Finished ->
        setf v B.Event.tag `Finished;
        ()
      
    in
    setf v B.Event.body body;
    v
    
  module B = B.Event
end