open Base

let size _frame = C.frame_size () |> Rect.of_c

let render_block _f ?style title area =
  let style_c =
    Option.map style ~f:(fun s -> s |> C.Types.Style.to_c |> Ctypes.addr)
  in
  let area_c = C.Types.Rect.to_c area in
  C.Fn.render_block style_c title area_c

let render_string _f ?style str area =
  let style_c =
    Option.map style ~f:(fun s -> s |> C.Types.Style.to_c |> Ctypes.addr)
  in
  let area_c = C.Types.Rect.to_c area in
  C.Fn.render_string style_c str area_c
