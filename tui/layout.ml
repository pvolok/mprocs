open Base

type constr = C.Types.Constraint.t

type dir = C.Types.Direction.t

let split spec dir area =
  let len = Array.length spec in

  let spec_c =
    Array.map spec ~f:C.Types.Constraint.to_c
    |> Array.to_list
    |> Ctypes.CArray.of_list C.Constraint.t
    |> Ctypes.CArray.start
  in
  let len_c = Unsigned.Size_t.of_int len in
  let dir_c = C.Types.Direction.to_c dir in
  let area_c = C.Types.Rect.to_c area in
  let result_arr = Ctypes.CArray.make C.Rect.t len in
  let result_c = Ctypes.CArray.start result_arr in
  C.Fn.layout spec_c len_c dir_c area_c result_c;

  let result =
    Ctypes.CArray.to_list result_arr |> Array.of_list_map ~f:C.Types.Rect.of_c
  in
  result

let hsplit spec area = split spec Horizontal area
let vsplit spec area = split spec Vertical area
