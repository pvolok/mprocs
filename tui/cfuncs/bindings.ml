open Ctypes

module Make (F : Cstubs.FOREIGN) = struct
  module T = Tui_ctypes.Types_bindings.Make (Tui_ctypes.Types_stubs)

  open F
  include T

  let term_create = F.(foreign "tui_terminal_create" (void @-> returning void))
  let term_destroy =
    F.(foreign "tui_terminal_destroy" (void @-> returning void))
  let enable_raw_mode =
    F.(foreign "tui_enable_raw_mode" (void @-> returning void))
  let disable_raw_mode =
    F.(foreign "tui_disable_raw_mode" (void @-> returning void))
  let clear = F.(foreign "tui_clear" (void @-> returning void))
  let enter_alternate_screen =
    F.(foreign "tui_enter_alternate_screen" (void @-> returning void))
  let leave_alternate_screen =
    F.(foreign "tui_leave_alternate_screen" (void @-> returning void))

  let frame_size = F.(foreign "tui_frame_size" (void @-> returning T.Rect.t))

  let layout =
    F.(
      foreign "tui_layout"
        (ptr T.Constraint.t @-> size_t @-> T.Direction.t @-> T.Rect.t
       @-> ptr T.Rect.t @-> returning void))

  let render_start = F.(foreign "tui_render_start" (void @-> returning void))
  let render_end = F.(foreign "tui_render_start" (void @-> returning void))

  let render_start = F.(foreign "tui_render_start" (void @-> returning void))
  let render_end = F.(foreign "tui_render_end" (void @-> returning void))

  let render_block =
    F.(
      foreign "tui_render_block"
        (ptr_opt T.Style.t @-> string @-> T.Rect.t @-> returning void))

  let render_string =
    F.(
      foreign "tui_render_string"
        (ptr_opt T.Style.t @-> string @-> T.Rect.t @-> returning void))

  let mod_bold = foreign_value "tui_mod_bold" uint16_t
  let mod_dim = foreign_value "tui_mod_dim" uint16_t
  let mod_italic = foreign_value "tui_mod_italic" uint16_t
  let mod_underlined = foreign_value "tui_mod_underlined" uint16_t
  let mod_slow_blink = foreign_value "tui_mod_slow_blink" uint16_t
  let mod_rapid_blink = foreign_value "tui_mod_rapid_blink" uint16_t
  let mod_reversed = foreign_value "tui_mod_reversed" uint16_t
  let mod_hidden = foreign_value "tui_mod_hidden" uint16_t
  let mod_crossed_out = foreign_value "tui_mod_crossed_out" uint16_t

  let mod_empty = foreign_value "tui_mod_empty" uint16_t
end

module Events (F : Cstubs.FOREIGN) = struct
  module T = Tui_ctypes.Types_bindings.Make (Tui_ctypes.Types_stubs)

  let tui_events_read_rs =
    F.(foreign "tui_events_read" (void @-> returning T.Event.t))
end
