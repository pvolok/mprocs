type t = Types.terminal

external create : unit -> t = "tui_terminal_create"

external enable_raw_mode : unit -> unit = "tui_terminal_enable_raw_mode"
external disable_raw_mode : unit -> unit = "tui_terminal_disable_raw_mode"

external clear : t -> unit = "tui_terminal_clear"

external enter_alternate_screen : unit -> unit = "tui_enter_alternate_screen"
external leave_alternate_screen : unit -> unit = "tui_leave_alternate_screen"
