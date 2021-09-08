type t = T of (unit -> unit) list

let empty = T []

let add (T lst) f = T (f :: lst)

let dispose (T lst) = List.iter (fun f -> f ()) lst
