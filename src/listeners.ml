type 'a listener = 'a -> unit
type 'a t = (int, 'a listener) Hashtbl.t
type id = Id of int

let last_id = ref 0

let create () = Hashtbl.create 2

let add t f =
  let id =
    last_id := !last_id + 1;
    !last_id
  in
  Hashtbl.add t id f;
  Id id

let add_once t f =
  let id =
    last_id := !last_id + 1;
    !last_id
  in
  Hashtbl.add t id (fun v ->
      Hashtbl.remove t id;
      f v);
  Id id

let rem t (Id id) = Hashtbl.remove t id

let addl t f dispose =
  let id = add t f in
  Dispose.add dispose (fun () -> rem t id)

let call t arg = Hashtbl.iter (fun _ f -> f arg) t
