open Base
open Ir
open Caml.Format

let print_bindings f items =
  pp_open_vbox f 0;
  fprintf f "open T@;@;";

  pp_print_list
    (fun f -> function
      | Ir.Struct ({ s_name; fields; _ } as _struc) ->
          let name = Id.str s_name in

          fprintf f "module %s = " (Id.pascal s_name);
          Fmt.wrap_block f "struct" "end" (fun () ->
              fprintf f "type t@;";
              fprintf f "let t : t structure typ = structure \"%s\"@;" name;
              List.iter fields ~f:(fun { f_name; f_type; _ } ->
                  fprintf f "let %s = field t \"%s\" %s@;" (Id.str f_name)
                    (Id.str f_name) (Type.ml_c f_type));
              fprintf f "let () = seal t");
          fprintf f "@;"
      | Ir.Variant ({ v_name; ctors; _ } as _variant) ->
          let name = Id.str v_name in

          fprintf f "module %s = " (Id.pascal v_name);
          Fmt.wrap_block f "struct" "end" (fun () ->
              (* Tag *)
              Fmt.wrap_block f "module Tag = struct" "end" (fun () ->
                  fprintf f "type t = [@ @[<v>";
                  pp_print_list
                    (fun f { c_name; _ } ->
                      fprintf f "| `%s" (Id.pascal c_name))
                    f ctors;
                  fprintf f "@]@;<1 0>]@;@;";

                  List.iter ctors ~f:(fun { c_name; _ } ->
                      fprintf f "let %s = constant \"%s_%s\" int64_t@;"
                        (Id.str c_name) (Id.str v_name) (Id.str c_name));
                  fprintf f "@;@;";

                  fprintf f "let t : t typ = enum \"%s_tag\" [@;<1 2>@[<v>" name;
                  pp_print_list
                    (fun f { c_name; _ } ->
                      fprintf f "`%s, %s;" (Id.pascal c_name) (Id.str c_name))
                    f ctors;
                  fprintf f "@]@;<1 0>]");

              fprintf f "@;@;";

              (* Body *)
              Fmt.wrap_block f "module Body = struct" "end" (fun () ->
                  fprintf f "type t@;";
                  fprintf f "let t : t union typ = union \"%s_body\"@;" name;
                  pp_print_list
                    (fun f { c_name; args; _ } ->
                      pp_print_list
                        (fun f (i, arg) ->
                          fprintf f "let %s_%d = field t \"%s_%d\" %s"
                            (Id.str c_name) i (Id.str c_name) i (Type.ml_c arg))
                        f
                        (List.mapi args ~f:(fun i arg -> (i, arg))))
                    f ctors;

                  fprintf f "@;let () = seal t";

                  ());
              fprintf f "@;@;";

              (* Type *)
              fprintf f "type t@;";
              fprintf f "let t : t structure typ = structure \"%s\"@;" name;
              fprintf f "let tag = field t \"tag\" Tag.t@;";
              fprintf f "let body = field t \"body\" Body.t@;";
              fprintf f "let () = seal t@;";

              ())
      | Fn _ -> ())
    f items;

  pp_close_box f ()

let print_bindings f items =
  fprintf f "@[<v>open Ctypes@;@;";
  fprintf f "module Make (T : Cstubs_structs.TYPE) = struct@;<1 2>@[<v>";
  print_bindings f items;
  fprintf f "@]@;end@;";
  fprintf f "@]";

  ()

let print_types f items =
  pp_open_vbox f 0;
  fprintf f "module B = Types_bindings.Make (Types_stubs)@;@;";

  pp_print_list
    (fun f -> function
      | Ir.Struct ({ s_name; fields; _ } as _struc) ->
          fprintf f "module %s = " (Id.pascal s_name);
          Fmt.wrap_block f "struct" "end" (fun () ->
              Fmt.wrap_list f "type t = {" "}" fields
                (fun f { f_name; f_type; _ } ->
                  fprintf f "%s : %s;" (Id.str f_name) (Type.ml f_type));
              pp_print_string f
                "[@@deriving compare, hash, sexp_of,\n              show]";
              fprintf f "@;@;";

              Fmt.wrap_block f "let of_c x =" "" (fun () ->
                  Fmt.wrap_list f "{" "}" fields (fun f { f_name; f_type; _ } ->
                      fprintf f "%s = getf x B.%s.%s |> %s;" (Id.str f_name)
                        (Id.pascal s_name) (Id.str f_name) (Type.ml_of_c f_type)));
              fprintf f "@;@;";

              Fmt.wrap_block f "let to_c x =" "" (fun () ->
                  fprintf f "let v = make B.%s.t in@;" (Id.pascal s_name);
                  List.iter fields ~f:(fun { f_name; f_type; _ } ->
                      fprintf f "setf v B.%s.%s (%s x.%s);@;" (Id.pascal s_name)
                        (Id.str f_name) (Type.ml_to_c f_type) (Id.str f_name));
                  fprintf f "v@;");
              fprintf f "@;";

              ());
          fprintf f "@;"
      | Ir.Variant ({ v_name; ctors; _ } as _variant) ->
          let name = Id.str v_name in

          fprintf f "module %s = " (Id.pascal v_name);
          Fmt.wrap_block f "struct" "end" (fun () ->
              (* type t *)
              Fmt.wrap_list f "type t =" "" ctors (fun f { c_name; args; _ } ->
                  fprintf f "| %s" (Id.pascal c_name);
                  (match args with
                  | [] -> ()
                  | args ->
                      fprintf f " of ";
                      pp_print_list
                        ~pp_sep:(fun f () -> pp_print_string f " * ")
                        (fun f ctor -> fprintf f "%s" (Type.ml ctor))
                        f args);
                  ());
              pp_print_string f
                "[@@deriving compare, hash, sexp_of,\n              show]";
              fprintf f "@;";

              (* of_c *)
              Fmt.wrap_block f "let of_c x =" "" (fun () ->
                  fprintf f "let body = getf x B.%s.body in@;"
                    (Id.pascal v_name);
                  fprintf f "match getf x B.%s.tag with@;" (Id.pascal v_name);
                  List.iter ctors ~f:(fun { c_name; args; _ } ->
                      fprintf f "| `%s ->@;" (Id.pascal c_name);
                      fprintf f "  %s" (Id.pascal c_name);

                      (match args with
                      | [] -> ()
                      | args ->
                          fprintf f " (";
                          pp_open_box f 0;
                          pp_print_list
                            ~pp_sep:(fun f () -> fprintf f ",@ ")
                            (fun f (i, arg) ->
                              fprintf f "getf body B.%s.Body.%s_%d |> %s"
                                (Id.pascal v_name) (Id.str c_name) i
                                (Type.ml_of_c arg))
                            f
                            (List.mapi args ~f:(fun i arg -> (i, arg)));
                          pp_close_box f ();
                          fprintf f ")");
                      fprintf f "@;");
                  ());

              (* to_c *)
              Fmt.wrap_block f "let to_c x =" "" (fun () ->
                  fprintf f "let v = make B.%s.t in@;" (Id.pascal v_name);
                  fprintf f "let body = make B.%s.Body.t in@;"
                    (Id.pascal v_name);
                  Fmt.wrap_block f "let () = " "in" (fun () ->
                      fprintf f "match x with@;";
                      List.iter ctors ~f:(fun { c_name; args; _ } ->
                          fprintf f "| %s" (Id.pascal c_name);

                          if not (List.is_empty args) then (
                            fprintf f " (";
                            pp_open_box f 0;
                            pp_print_list
                              ~pp_sep:(fun f () -> fprintf f ",@ ")
                              (fun f (i, _) -> fprintf f "a%d" i)
                              f
                              (List.mapi args ~f:(fun i arg -> (i, arg)));
                            pp_close_box f ();
                            fprintf f ")");
                          fprintf f " ->";

                          Fmt.wrap_block f "" "" (fun () ->
                              fprintf f "setf v B.%s.tag `%s;@;"
                                (Id.pascal v_name) (Id.pascal c_name);
                              List.iteri args ~f:(fun i arg ->
                                  fprintf f
                                    "setf body B.%s.Body.%s_%d (%s a%d);@;"
                                    (Id.pascal v_name) (Id.str c_name) i
                                    (Type.ml_to_c arg) i);
                              fprintf f "()";
                              ())));
                  fprintf f "@;";
                  fprintf f "setf v B.%s.body body;@;" (Id.pascal v_name);
                  fprintf f "v@;";
                  ());

              fprintf f "module B = B.%s" (Id.pascal v_name);
              ())
      | Fn _ -> ())
    f items;

  pp_close_box f ()

let print_types f items =
  fprintf f "@[<v>open Ctypes@;@;";
  print_types f items;
  fprintf f "@]";

  ()
