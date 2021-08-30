
type os =
    | Windows
    | Mac
    | Linux
    | Unknown

let uname () =
    let ic = Unix.open_process_in "uname" in
    let uname = input_line ic in
    let () = close_in ic in
    uname;;

let get_os =
    match Sys.os_type with
    | "Win32" -> Windows
    | _ -> match uname () with
        | "Darwin" -> Mac
        | "Linux" -> Linux
        | _ -> Unknown

let cclib lib = ["-cclib"; lib]

let flags =
    match get_os with
    | Linux -> []
        @ cclib("-lutil")
    | _ -> []
;;

let c_library_flags =
    match get_os with
    | Linux -> ["-lutil"]
    | _ -> []
;;

Configurator.V1.Flags.write_sexp "flags.sexp" flags;
Configurator.V1.Flags.write_sexp "c_library_flags.sexp" c_library_flags;
