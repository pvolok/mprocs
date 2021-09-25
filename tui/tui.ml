(* Init *)

external tui_init : unit -> unit = "tui_init"

let () = tui_init ()

(* End init *)

module Event = Event
module Events = Events
module Layout = Layout
module Term = Terminal
module Rect = Rect
module Render = Render
module Style = Style
module Types = Types

type frame = Types.frame

let create () =
  Term.create ();
  Term.enable_raw_mode ();
  Term.enter_alternate_screen ();
  Term.clear ()

let destroy () =
  Term.leave_alternate_screen ();
  Term.disable_raw_mode ();
  Term.destroy ()

let clear term = Term.clear term

let render = Render.render
let render_block = Render.render_block
let render_string = Render.render_string
