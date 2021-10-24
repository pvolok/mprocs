include C.Types.Style

module Color = C.Types.Color
module ColorOpt = C.Types.ColorOpt

module Mod = struct
  type t = int

  open Ctypes

  let bold = !@C.Fn.mod_bold |> Unsigned.UInt16.to_int
  let dim = !@C.Fn.mod_dim |> Unsigned.UInt16.to_int
  let italic = !@C.Fn.mod_italic |> Unsigned.UInt16.to_int
  let underlined = !@C.Fn.mod_underlined |> Unsigned.UInt16.to_int
  let slow_blink = !@C.Fn.mod_slow_blink |> Unsigned.UInt16.to_int
  let rapid_blink = !@C.Fn.mod_rapid_blink |> Unsigned.UInt16.to_int
  let reversed = !@C.Fn.mod_reversed |> Unsigned.UInt16.to_int
  let hidden = !@C.Fn.mod_hidden |> Unsigned.UInt16.to_int
  let crossed_out = !@C.Fn.mod_crossed_out |> Unsigned.UInt16.to_int

  let empty = !@C.Fn.mod_empty |> Unsigned.UInt16.to_int
end

let make ?fg ?bg ?(mods = Mod.empty) ?(sub_mods = Mod.empty) () =
  {
    fg =
      (match fg with
      | None -> ColorOpt.None
      | Some color -> ColorOpt.Some color);
    bg =
      (match bg with
      | None -> ColorOpt.None
      | Some color -> ColorOpt.Some color);
    add_modifier = mods;
    sub_modifier = sub_mods;
  }
