open Ctypes

module Make (T : Cstubs_structs.TYPE) = struct
  open T
  
  module Rect = struct
    type t
    let t : t structure typ = structure "rect"
    let x = field t "x" uint16_t
    let y = field t "y" uint16_t
    let w = field t "w" uint16_t
    let h = field t "h" uint16_t
    let () = seal t
  end
  
  module Constraint = struct
    module Tag = struct
      type t = [
      | `Percentage
      | `Ratio
      | `Length
      | `Min
      | `Max
      ]
      
      let percentage = constant "constraint_percentage" int64_t
      let ratio = constant "constraint_ratio" int64_t
      let length = constant "constraint_length" int64_t
      let min = constant "constraint_min" int64_t
      let max = constant "constraint_max" int64_t
      
      
      let t : t typ = enum "constraint_tag" [
        `Percentage, percentage;
        `Ratio, ratio;
        `Length, length;
        `Min, min;
        `Max, max;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "constraint_body"
      let percentage_0 = field t "percentage_0" uint16_t
      let ratio_0 = field t "ratio_0" uint32_t
      let ratio_1 = field t "ratio_1" uint32_t
      let length_0 = field t "length_0" uint16_t
      let min_0 = field t "min_0" uint16_t
      let max_0 = field t "max_0" uint16_t
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "constraint"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module Direction = struct
    module Tag = struct
      type t = [
      | `Horizontal
      | `Vertical
      ]
      
      let horizontal = constant "direction_horizontal" int64_t
      let vertical = constant "direction_vertical" int64_t
      
      
      let t : t typ = enum "direction_tag" [
        `Horizontal, horizontal;
        `Vertical, vertical;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "direction_body"
      
      
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "direction"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module Color = struct
    module Tag = struct
      type t = [
      | `Reset
      | `Black
      | `Red
      | `Green
      | `Yellow
      | `Blue
      | `Magenta
      | `Cyan
      | `Gray
      | `DarkGray
      | `LightRed
      | `LightGreen
      | `LightYellow
      | `LightBlue
      | `LightMagenta
      | `LightCyan
      | `White
      | `Rgb
      | `Indexed
      ]
      
      let reset = constant "color_reset" int64_t
      let black = constant "color_black" int64_t
      let red = constant "color_red" int64_t
      let green = constant "color_green" int64_t
      let yellow = constant "color_yellow" int64_t
      let blue = constant "color_blue" int64_t
      let magenta = constant "color_magenta" int64_t
      let cyan = constant "color_cyan" int64_t
      let gray = constant "color_gray" int64_t
      let dark_gray = constant "color_dark_gray" int64_t
      let light_red = constant "color_light_red" int64_t
      let light_green = constant "color_light_green" int64_t
      let light_yellow = constant "color_light_yellow" int64_t
      let light_blue = constant "color_light_blue" int64_t
      let light_magenta = constant "color_light_magenta" int64_t
      let light_cyan = constant "color_light_cyan" int64_t
      let white = constant "color_white" int64_t
      let rgb = constant "color_rgb" int64_t
      let indexed = constant "color_indexed" int64_t
      
      
      let t : t typ = enum "color_tag" [
        `Reset, reset;
        `Black, black;
        `Red, red;
        `Green, green;
        `Yellow, yellow;
        `Blue, blue;
        `Magenta, magenta;
        `Cyan, cyan;
        `Gray, gray;
        `DarkGray, dark_gray;
        `LightRed, light_red;
        `LightGreen, light_green;
        `LightYellow, light_yellow;
        `LightBlue, light_blue;
        `LightMagenta, light_magenta;
        `LightCyan, light_cyan;
        `White, white;
        `Rgb, rgb;
        `Indexed, indexed;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "color_body"
      
      
      
      
      
      
      
      
      
      
      
      
      
      
      
      
      
      let rgb_0 = field t "rgb_0" uint8_t
      let rgb_1 = field t "rgb_1" uint8_t
      let rgb_2 = field t "rgb_2" uint8_t
      let indexed_0 = field t "indexed_0" uint8_t
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "color"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module ColorOpt = struct
    module Tag = struct
      type t = [
      | `Some
      | `None
      ]
      
      let some = constant "color_opt_some" int64_t
      let none = constant "color_opt_none" int64_t
      
      
      let t : t typ = enum "color_opt_tag" [
        `Some, some;
        `None, none;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "color_opt_body"
      let some_0 = field t "some_0" Color.t
      
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "color_opt"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module Style = struct
    type t
    let t : t structure typ = structure "style"
    let fg = field t "fg" ColorOpt.t
    let bg = field t "bg" ColorOpt.t
    let add_modifier = field t "add_modifier" uint16_t
    let sub_modifier = field t "sub_modifier" uint16_t
    let () = seal t
  end
  
  module KeyCode = struct
    module Tag = struct
      type t = [
      | `Backspace
      | `Enter
      | `Left
      | `Right
      | `Up
      | `Down
      | `Home
      | `End
      | `PageUp
      | `PageDown
      | `Tab
      | `BackTab
      | `Delete
      | `Insert
      | `F
      | `Char
      | `Null
      | `Esc
      ]
      
      let backspace = constant "key_code_backspace" int64_t
      let enter = constant "key_code_enter" int64_t
      let left = constant "key_code_left" int64_t
      let right = constant "key_code_right" int64_t
      let up = constant "key_code_up" int64_t
      let down = constant "key_code_down" int64_t
      let home = constant "key_code_home" int64_t
      let end_ = constant "key_code_end_" int64_t
      let page_up = constant "key_code_page_up" int64_t
      let page_down = constant "key_code_page_down" int64_t
      let tab = constant "key_code_tab" int64_t
      let back_tab = constant "key_code_back_tab" int64_t
      let delete = constant "key_code_delete" int64_t
      let insert = constant "key_code_insert" int64_t
      let f = constant "key_code_f" int64_t
      let char = constant "key_code_char" int64_t
      let null = constant "key_code_null" int64_t
      let esc = constant "key_code_esc" int64_t
      
      
      let t : t typ = enum "key_code_tag" [
        `Backspace, backspace;
        `Enter, enter;
        `Left, left;
        `Right, right;
        `Up, up;
        `Down, down;
        `Home, home;
        `End, end_;
        `PageUp, page_up;
        `PageDown, page_down;
        `Tab, tab;
        `BackTab, back_tab;
        `Delete, delete;
        `Insert, insert;
        `F, f;
        `Char, char;
        `Null, null;
        `Esc, esc;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "key_code_body"
      
      
      
      
      
      
      
      
      
      
      
      
      
      
      let f_0 = field t "f_0" uint8_t
      let char_0 = field t "char_0" uint32_t
      
      
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "key_code"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module KeyMods = struct
    type t
    let t : t structure typ = structure "key_mods"
    let shift = field t "shift" uint8_t
    let control = field t "control" uint8_t
    let alt = field t "alt" uint8_t
    let () = seal t
  end
  
  module KeyEvent = struct
    type t
    let t : t structure typ = structure "key_event"
    let code = field t "code" KeyCode.t
    let modifiers = field t "modifiers" KeyMods.t
    let () = seal t
  end
  
  module MouseButton = struct
    module Tag = struct
      type t = [
      | `Left
      | `Right
      | `Middle
      ]
      
      let left = constant "mouse_button_left" int64_t
      let right = constant "mouse_button_right" int64_t
      let middle = constant "mouse_button_middle" int64_t
      
      
      let t : t typ = enum "mouse_button_tag" [
        `Left, left;
        `Right, right;
        `Middle, middle;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "mouse_button_body"
      
      
      
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "mouse_button"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module MouseEventKind = struct
    module Tag = struct
      type t = [
      | `Down
      | `Up
      | `Drag
      | `Moved
      | `ScrollDown
      | `ScrollUp
      ]
      
      let down = constant "mouse_event_kind_down" int64_t
      let up = constant "mouse_event_kind_up" int64_t
      let drag = constant "mouse_event_kind_drag" int64_t
      let moved = constant "mouse_event_kind_moved" int64_t
      let scroll_down = constant "mouse_event_kind_scroll_down" int64_t
      let scroll_up = constant "mouse_event_kind_scroll_up" int64_t
      
      
      let t : t typ = enum "mouse_event_kind_tag" [
        `Down, down;
        `Up, up;
        `Drag, drag;
        `Moved, moved;
        `ScrollDown, scroll_down;
        `ScrollUp, scroll_up;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "mouse_event_kind_body"
      let down_0 = field t "down_0" MouseButton.t
      let up_0 = field t "up_0" MouseButton.t
      let drag_0 = field t "drag_0" MouseButton.t
      
      
      
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "mouse_event_kind"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
  module MouseEvent = struct
    type t
    let t : t structure typ = structure "mouse_event"
    let kind = field t "kind" MouseEventKind.t
    let column = field t "column" uint16_t
    let row = field t "row" uint16_t
    let modifiers = field t "modifiers" KeyMods.t
    let () = seal t
  end
  
  module Event = struct
    module Tag = struct
      type t = [
      | `Key
      | `Mouse
      | `Resize
      | `Finished
      ]
      
      let key = constant "event_key" int64_t
      let mouse = constant "event_mouse" int64_t
      let resize = constant "event_resize" int64_t
      let finished = constant "event_finished" int64_t
      
      
      let t : t typ = enum "event_tag" [
        `Key, key;
        `Mouse, mouse;
        `Resize, resize;
        `Finished, finished;
      ]
    end
    
    module Body = struct
      type t
      let t : t union typ = union "event_body"
      let key_0 = field t "key_0" KeyEvent.t
      let mouse_0 = field t "mouse_0" MouseEvent.t
      let resize_0 = field t "resize_0" uint16_t
      let resize_1 = field t "resize_1" uint16_t
      
      let () = seal t
    end
    
    type t
    let t : t structure typ = structure "event"
    let tag = field t "tag" Tag.t
    let body = field t "body" Body.t
    let () = seal t
    
  end
end
