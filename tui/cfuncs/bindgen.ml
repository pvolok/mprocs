let () =
  let prefix = "tui_" in

  let stubs_oc = open_out "funcs_stubs.c" in
  let fmt = Format.formatter_of_out_channel stubs_oc in

  let write_include name =
    Format.pp_print_string fmt ("#include \"" ^ name ^ "\"");
    Format.pp_print_newline fmt ()
  in

  write_include "funcs.h";

  Cstubs.write_c fmt ~prefix (module Bindings.Make);
  Cstubs.write_c fmt ~prefix ~concurrency:Cstubs.lwt_jobs
    (module Bindings.Events);
  close_out stubs_oc;

  let () =
    let generated_oc = open_out "funcs_stubs.ml" in
    let fmt = Format.formatter_of_out_channel generated_oc in
    Cstubs.write_ml fmt ~prefix (module Bindings.Make);
    close_out generated_oc
  in

  let () =
    let generated_oc = open_out "funcs_stubs2.ml" in
    let fmt = Format.formatter_of_out_channel generated_oc in
    Cstubs.write_ml fmt ~prefix ~concurrency:Cstubs.lwt_jobs
      (module Bindings.Events);
    close_out generated_oc
  in
  ()
