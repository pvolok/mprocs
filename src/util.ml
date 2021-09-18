let trim len s = if String.length s > len then String.sub s 0 (max 0 len) else s

let block_style active =
  if active then Tui.Style.(make ~fg:Reset ~mods:Mod.bold ())
  else Tui.Style.(make ~fg:(Rgb (128, 128, 128)) ())
