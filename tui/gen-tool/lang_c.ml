open Base
open Ir
open Caml.Format

let headers =
  {|
#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
|}

let print_type f typ = fprintf f "%s" (Type.c typ)

let print f items =
  pp_open_vbox f 0;

  fprintf f "%s@;@;" headers;

  pp_print_list
    (fun f -> function
      | Ir.Struct ({ s_name; fields; _ } as _struc) ->
          fprintf f "typedef struct %s {@;<1 2>@[<v>" (Id.str s_name);
          pp_print_list
            (fun f { f_name; f_type; _ } ->
              fprintf f "%a %s;" print_type f_type (Id.str f_name))
            f fields;
          fprintf f "@]@;<1 0>} %s;@;@;" (Id.str s_name)
      | Ir.Variant ({ v_name; ctors; _ } as _variant) ->
          fprintf f "typedef enum %s_tag {@;<1 2>@[<v>" (Id.str v_name);
          pp_print_list
            (fun f { c_name; _ } ->
              fprintf f "%s_%s," (Id.str v_name) (Id.str c_name))
            f ctors;
          fprintf f "@]@;<1 0>} %s_tag;@;@;" (Id.str v_name);

          fprintf f "typedef union %s_body {@;<1 2>@[<v>" (Id.str v_name);
          pp_print_list
            (fun f { c_name; args; _ } ->
              fprintf f "struct {@;<1 2>@[<v>";
              pp_print_list
                (fun f (i, arg) ->
                  fprintf f "%s %s_%d;" (Type.c arg) (Id.str c_name) i)
                f
                (List.mapi args ~f:(fun i arg -> (i, arg)));
              fprintf f "@]@;<1 0>};")
            f ctors;
          fprintf f "@]@;<1 0>} %s_body;@;@;" (Id.str v_name);

          fprintf f "typedef struct %s {@;<1 2>@[<v>" (Id.str v_name);
          fprintf f "%s_tag tag;@ " (Id.str v_name);
          fprintf f "%s_body body;" (Id.str v_name);
          fprintf f "@]@;<1 0>} %s;@;@;" (Id.str v_name);

          ()
      | Ir.Fn _ -> ())
    f items;

  pp_close_box f ()
