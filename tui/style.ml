type color =
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

module Mod = struct
  type t = int

  module Internal = struct
    external get_bold : unit -> t = "tui_style_bold"
    external get_dim : unit -> t = "tui_style_dim"
    external get_italic : unit -> t = "tui_style_italic"
    external get_underlined : unit -> t = "tui_style_underlined"
    external get_slow_blink : unit -> t = "tui_style_slow_blink"
    external get_rapid_blink : unit -> t = "tui_style_rapid_blink"
    external get_reversed : unit -> t = "tui_style_reversed"
    external get_hidden : unit -> t = "tui_style_hidden"
    external get_crossed_out : unit -> t = "tui_style_crossed_out"
  end
  open Internal

  let bold = get_bold ()
  let dim = get_dim ()
  let italic = get_italic ()
  let underlined = get_underlined ()
  let slow_blink = get_slow_blink ()
  let rapid_blink = get_rapid_blink ()
  let reversed = get_reversed ()
  let hidden = get_hidden ()
  let crossed_out = get_crossed_out ()

  let empty = 0
end

type t = {
  fg : color option;
  bg : color option;
  add_modifier : Mod.t;
  sub_modifier : Mod.t;
}

let make ?fg ?bg ?(mods = Mod.empty) ?(sub_mods = Mod.empty) () =
  { fg; bg; add_modifier = mods; sub_modifier = sub_mods }
