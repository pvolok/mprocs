open Types

type constr =
  | Percentage of int
  | Ratio of int * int
  | Length of int
  | Max of int
  | Min of int

type dir =
  | Horizontal
  | Vertical

external split : constr array -> dir -> Rect.t -> Rect.t array = "tui_layout"

let hsplit spec area = split spec Horizontal area
let vsplit spec area = split spec Vertical area
