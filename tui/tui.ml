module C = C
module Event = Event
module Events = Events
module Layout = Layout
module Rect = Rect
module Render = Render
module Style = Style

let create () =
  C.Fn.term_create ();
  C.Fn.enable_raw_mode ();
  C.Fn.enter_alternate_screen ();
  C.Fn.clear ();
  ()

let destroy () =
  C.Fn.leave_alternate_screen ();
  C.Fn.disable_raw_mode ();
  C.Fn.term_destroy ();
  ()

let clear term = C.Fn.clear term

let render_start = C.Fn.render_start
let render_end = C.Fn.render_end
let render_block = Render.render_block
let render_string = Render.render_string
