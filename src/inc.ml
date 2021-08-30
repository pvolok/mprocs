module Inc = Incremental.Make ()
include Inc

module Map = Incr_map.Make (Inc)
