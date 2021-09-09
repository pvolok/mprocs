open StdLabels
open Notty
open Nottui

open Lwd_infix

type constr =
  | Fixed of int
  | Fill

let calc_layout len parts =
  let for_fixed, fill_count =
    List.fold_left parts ~init:(0, 0)
      ~f:(fun (for_fixed, fill_count) (constr, _) ->
        match constr with
        | Fixed n -> (for_fixed + n, fill_count)
        | Fill -> (for_fixed, fill_count + 1))
  in
  let per_fill = (len - for_fixed) / fill_count in
  let _, acc =
    List.fold_left parts ~init:(len, []) ~f:(fun (len, acc) (constr, ui) ->
        let part_len = match constr with Fixed n -> n | Fill -> per_fill in
        let part_len = min len part_len in
        (len - part_len, (part_len, ui) :: acc))
  in
  List.rev acc

let hor (wsize : (int * int) Lwd.t) parts =
  let wsizes =
    let$ w, h = wsize in
    calc_layout w parts |> Array.of_list |> Array.map ~f:(fun (l, _) -> (l, h))
  in
  let parts =
    List.mapi parts ~f:(fun i (_, part) ->
        part (Lwd.map wsizes ~f:(fun sizes -> sizes.(i))))
  in
  Lwd_utils.pack Ui.pack_x parts

let vert (wsize : (int * int) Lwd.t) parts =
  let wsizes =
    let$ w, h = wsize in
    calc_layout h parts |> Array.of_list |> Array.map ~f:(fun (l, _) -> (w, l))
  in
  let parts =
    List.mapi parts ~f:(fun i (_, part) ->
        part (Lwd.map wsizes ~f:(fun sizes -> sizes.(i))))
  in
  Lwd_utils.pack Ui.pack_y parts
