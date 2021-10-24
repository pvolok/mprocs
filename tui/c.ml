module Types = Tui_ctypes.Types

module Fn = Funcs_bindings.Make (Funcs_stubs)
include Fn

module Fn2 = Funcs_bindings.Events (Funcs_stubs2)
include Fn
