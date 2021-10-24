open Ir

let defs =
  let color =
    Type.make ~c:"color" ~ml:"Color.t" ~ml_c:"Color.t" ~ml_of_c:"Color.of_c"
      ~ml_to_c:"Color.to_c" ~rs:"Color" ()
  in
  let color_opt =
    Type.make ~c:"color_opt" ~ml:"ColorOpt.t" ~ml_c:"ColorOpt.t"
      ~ml_of_c:"ColorOpt.of_c" ~ml_to_c:"ColorOpt.to_c" ~rs:"ColorOpt" ()
  in

  let key_code =
    Type.make ~c:"key_code" ~ml:"KeyCode.t" ~ml_c:"KeyCode.t"
      ~ml_of_c:"KeyCode.of_c" ~ml_to_c:"KeyCode.to_c" ~rs:"KeyCode" ()
  in
  let key_mods =
    Type.make ~c:"key_mods" ~ml:"KeyMods.t" ~ml_c:"KeyMods.t"
      ~ml_of_c:"KeyMods.of_c" ~ml_to_c:"KeyMods.to_c" ~rs:"KeyMods" ()
  in
  let key_event =
    Type.make ~c:"key_event" ~ml:"KeyEvent.t" ~ml_c:"KeyEvent.t"
      ~ml_of_c:"KeyEvent.of_c" ~ml_to_c:"KeyEvent.to_c" ~rs:"KeyEvent" ()
  in
  let mouse_button =
    Type.make ~c:"mouse_button" ~ml:"MouseButton.t" ~ml_c:"MouseButton.t"
      ~ml_of_c:"MouseButton.of_c" ~ml_to_c:"MouseButton.to_c" ~rs:"MouseButton"
      ()
  in
  let mouse_event_kind =
    Type.make ~c:"mouse_event_kind" ~ml:"MouseEventKind.t"
      ~ml_c:"MouseEventKind.t" ~ml_of_c:"MouseEventKind.of_c"
      ~ml_to_c:"MouseEventKind.to_c" ~rs:"MouseEventKind" ()
  in
  let mouse_event =
    Type.make ~c:"mouse_event" ~ml:"MouseEvent.t" ~ml_c:"MouseEvent.t"
      ~ml_of_c:"MouseEvent.of_c" ~ml_to_c:"MouseEvent.to_c" ~rs:"MouseEvent" ()
  in

  [
    struc (id "rect") ~rs_name:"tui::layout::Rect"
      [
        field (id "x") uint16;
        field (id "y") uint16;
        field (id "w") ~rs_name:"width" uint16;
        field (id "h") ~rs_name:"height" uint16;
      ];
    variant (id "constraint") ~rs_name:"tui::layout::Constraint"
      [
        ctor (id "percentage") [ uint16 ];
        ctor (id "ratio") [ uint32; uint32 ];
        ctor (id "length") [ uint16 ];
        ctor (id "min") [ uint16 ];
        ctor (id "max") [ uint16 ];
      ];
    variant (id "direction") ~rs_name:"tui::layout::Direction"
      [ ctor (id "horizontal") []; ctor (id "vertical") [] ];
    variant (id "color") ~rs_name:"tui::style::Color"
      [
        ctor (id "reset") [];
        ctor (id "black") [];
        ctor (id "red") [];
        ctor (id "green") [];
        ctor (id "yellow") [];
        ctor (id "blue") [];
        ctor (id "magenta") [];
        ctor (id "cyan") [];
        ctor (id "gray") [];
        ctor (id "dark_gray") [];
        ctor (id "light_red") [];
        ctor (id "light_green") [];
        ctor (id "light_yellow") [];
        ctor (id "light_blue") [];
        ctor (id "light_magenta") [];
        ctor (id "light_cyan") [];
        ctor (id "white") [];
        ctor (id "rgb") [ uint8; uint8; uint8 ];
        ctor (id "indexed") [ uint8 ];
      ];
    variant (id "color_opt") [ ctor (id "some") [ color ]; ctor (id "none") [] ];
    struc (id "style")
      [
        field (id "fg") color_opt;
        field (id "bg") color_opt;
        field (id "add_modifier") uint16;
        field (id "sub_modifier") uint16;
      ];
    (* Event *)
    variant (id "key_code")
      [
        ctor (id "backspace") [];
        ctor (id "enter") [];
        ctor (id "left") [];
        ctor (id "right") [];
        ctor (id "up") [];
        ctor (id "down") [];
        ctor (id "home") [];
        ctor (id "end_") [];
        ctor (id "page_up") [];
        ctor (id "page_down") [];
        ctor (id "tab") [];
        ctor (id "back_tab") [];
        ctor (id "delete") [];
        ctor (id "insert") [];
        ctor (id "f") [ uint8 ];
        ctor (id "char") [ uint32 ];
        ctor (id "null") [];
        ctor (id "esc") [];
      ];
    struc (id "key_mods")
      [
        field (id "shift") uint8;
        field (id "control") uint8;
        field (id "alt") uint8;
      ];
    struc (id "key_event")
      [ field (id "code") key_code; field (id "modifiers") key_mods ];
    variant (id "mouse_button")
      [ ctor (id "left") []; ctor (id "right") []; ctor (id "middle") [] ];
    variant (id "mouse_event_kind")
      [
        ctor (id "down") [ mouse_button ];
        ctor (id "up") [ mouse_button ];
        ctor (id "drag") [ mouse_button ];
        ctor (id "moved") [];
        ctor (id "scroll_down") [];
        ctor (id "scroll_up") [];
      ];
    struc (id "mouse_event")
      [
        field (id "kind") mouse_event_kind;
        field (id "column") uint16;
        field (id "row") uint16;
        field (id "modifiers") key_mods;
      ];
    variant (id "event")
      [
        ctor (id "key") [ key_event ];
        ctor (id "mouse") [ mouse_event ];
        ctor (id "resize") [ uint16; uint16 ];
        ctor (id "finished") [];
      ];
  ]
