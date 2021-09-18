open Types

type constr =
  | Percentage of int
  | Ratio of int * int
  | Length of int
  | Max of int
  | Min of int

external split : constr array -> Rect.t -> Rect.t array = "tui_layout"
