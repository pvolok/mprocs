type t = T of (unit -> unit) list ref

let create () = T (ref [])

let add (T lst) f = lst := f :: !lst

let dispose (T lst) =
  let prev = !lst in
  lst := [];
  List.iter (fun f -> f ()) prev
