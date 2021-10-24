let () =
  let stubs_out = open_out "types_stubs_gen.c" in
  let stubs_fmt = Format.formatter_of_out_channel stubs_out in
  Format.fprintf stubs_fmt "%s@\n" "#include \"types.h\"";
  Cstubs_structs.write_c stubs_fmt (module Types_bindings.Stubs);
  Format.pp_print_flush stubs_fmt ();
  close_out stubs_out
