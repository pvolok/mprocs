module C = Configurator.V1

type os =
  | Android
  | IOS
  | Linux
  | Mac
  | Windows

let detect_system_header =
  {|
  #if __APPLE__
    #include <TargetConditionals.h>
    #if TARGET_OS_IPHONE
      #define PLATFORM_NAME "ios"
    #else
      #define PLATFORM_NAME "mac"
    #endif
  #elif __linux__
    #if __ANDROID__
      #define PLATFORM_NAME "android"
    #else
      #define PLATFORM_NAME "linux"
    #endif
  #elif WIN32
    #define PLATFORM_NAME "windows"
  #endif
|}

let () =
  C.main ~name:"discover" (fun t ->
      let platform =
        C.C_define.import t ~prelude:detect_system_header ~includes:[]
          [ ("PLATFORM_NAME", C.C_define.Type.String) ]
      in
      let os =
        match platform with
        | [ (_, String "linux") ] -> Linux
        | [ (_, String "mac") ] -> Mac
        | [ (_, String "windows") ] -> Windows
        | _ -> failwith "Unknown OS"
      in

      let flags =
        match os with
        | Mac -> []
        | Linux -> [ "-cclib"; "-static"; "-cclib"; "-no-pie" ]
        | Windows -> ["-cclib"; "-luserenv"]
        | _ -> failwith "Unsupported OS"
      in

      C.Flags.write_sexp "flags.sexp" flags;

      ())
