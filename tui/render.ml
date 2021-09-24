open Types

external size : frame -> Rect.t = "tui_render_frame_size"

(*external render_string : frame -> Style.t option -> string -> Rect.t -> unit*)
  (*= "tui_render_string"*)
external render_string : unit -> unit
  = "tui_render_string"
let render_string f ?style str area = ()

external render_block : frame -> Style.t option -> string -> Rect.t -> unit
  = "tui_render_block"
let render_block f ?style title area = render_block f style title area

external render : Types.terminal -> (frame -> unit) -> unit = "tui_render"
