open Core_kernel

type 'a listener = 'a -> unit
type 'a t = 'a listener Int.Table.t
type id = Id of int

let last_id = ref 0

let create () = Int.Table.create ()

let add t f =
  let id =
    last_id := !last_id + 1;
    !last_id
  in
  Hashtbl.add_exn t ~key:id ~data:f;
  Id id

let rem t (Id id) = Hashtbl.remove t id

let addl t f dispose =
  let id = add t f in
  Dispose.add dispose (fun () -> rem t id)

let call t arg = Hashtbl.iter t ~f:(fun f -> f arg)
