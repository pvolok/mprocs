let () =
  let () =
    let rs_out = open_out "types.rs" in
    let f = Format.formatter_of_out_channel rs_out in
    Lang_rust.print f Defs.defs;
    Format.pp_print_flush f ();
    close_out rs_out
  in

  let () =
    let ml_out = open_out "types.ml" in
    let f = Format.formatter_of_out_channel ml_out in
    Lang_ml.print_types f Defs.defs;
    Format.pp_print_flush f ();
    close_out ml_out
  in

  let () =
    let ml_out = open_out "types_bindings.ml" in
    let f = Format.formatter_of_out_channel ml_out in
    Lang_ml.print_bindings f Defs.defs;
    Format.pp_print_flush f ();
    close_out ml_out
  in

  let () =
    let h_out = open_out "types.h" in
    let f = Format.formatter_of_out_channel h_out in
    Lang_c.print f Defs.defs;
    Format.pp_print_flush f ();
    close_out h_out
  in

  ()
