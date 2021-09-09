open Notty
open Nottui

open Lwd_infix

let make ?title focused' f size' =
  let body =
    f (Lwd.map size' ~f:(fun (w, h) -> (max 0 (w - 2), max 0 (h - 2))))
  in
  let body = Lwd.map body ~f:(Ui.shift_area (-1) (-1)) in

  let title =
    match title with
    | Some title -> I.string title |> I.pad ~l:1
    | None -> I.empty
  in

  let uchr code = I.uchar (Uchar.of_int code) 1 1 in
  let frame =
    let$ w, h = size' and$ focused = focused' in
    let border =
      I.tabulate w h (fun x y ->
          match (x, y) with
          (* top left *)
          | 0, 0 -> uchr 0x256d
          (* top right *)
          | _, 0 when x = w - 1 -> uchr 0x256e
          (* top *)
          | _, 0 -> uchr 0x2500
          (* bottom left *)
          | 0, _ when y = h - 1 -> uchr 0x2570
          (* bottom right *)
          | _, _ when y = h - 1 && x = w - 1 -> uchr 0x256f
          (* bottom *)
          | _, _ when y = h - 1 -> uchr 0x2500
          (* left *)
          | 0, _ -> uchr 0x2502
          (* right *)
          | _, _ when x = w - 1 -> uchr 0x2502
          (* content *)
          | _, _ -> I.void 1 1)
    in
    let border = if focused then I.attr A.(fg green) border else border in
    Ui.atom I.(title </> border)
  in

  Lwd_utils.pack Ui.pack_z [ frame; body ]
