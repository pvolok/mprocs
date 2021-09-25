external create : unit -> unit = "tui_terminal_create"
external destroy : unit -> unit = "tui_terminal_destroy"

external enable_raw_mode : unit -> unit = "tui_terminal_enable_raw_mode"
external disable_raw_mode : unit -> unit = "tui_terminal_disable_raw_mode"

external clear : unit -> unit = "tui_terminal_clear"

external enter_alternate_screen : unit -> unit = "tui_enter_alternate_screen"
external leave_alternate_screen : unit -> unit = "tui_leave_alternate_screen"
