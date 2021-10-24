open Base
open Ir
open Caml.Format

let print_type f t = fprintf f "%s" (Type.rs t)

let print_impl_from_struct f struc =
  let { s_name; fields; s_rs_name } = struc in
  match s_rs_name with
  | None -> ()
  | Some rs_ident ->
      (* rust -> c *)
      fprintf f "impl From<%s> for %s {@;<1 2> @[<v>" rs_ident
        (Id.pascal s_name);
      fprintf f "fn from (x: %s) -> Self {@;<1 2> @[<v>" rs_ident;
      fprintf f "%s {@;<1 2> @[<v>" (Id.pascal s_name);
      pp_print_list
        (fun f { f_name; f_rs_name; f_type } ->
          let var =
            match f_rs_name with
            | Some rs_name -> rs_name
            | None -> Id.str f_name
          in
          fprintf f "%s: %s," (Id.str f_name) (Type.rs_to_c f_type ("x." ^ var)))
        f fields;
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}@;@;";

      (* c -> rust *)
      fprintf f "impl From<%s> for %s {@;<1 2> @[<v>" (Id.pascal s_name)
        rs_ident;
      fprintf f "fn from (x: %s) -> Self {@;<1 2> @[<v>" (Id.pascal s_name);
      fprintf f "%s {@;<1 2> @[<v>" rs_ident;
      pp_print_list
        (fun f { f_name; f_rs_name; f_type } ->
          let field_name =
            match f_rs_name with
            | Some rs_name -> rs_name
            | None -> Id.str f_name
          in
          let var = Id.str f_name in
          fprintf f "%s: %s," field_name (Type.rs_of_c f_type ("x." ^ var)))
        f fields;
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}@;@;";

      ()

let print_impl_from_variant f variant =
  let { v_name; ctors; v_rs_name } = variant in
  match v_rs_name with
  | None -> ()
  | Some rs_name ->
      (* rust -> c *)
      fprintf f "impl From<%s> for %s {@;<1 2> @[<v>" rs_name (Id.pascal v_name);
      fprintf f "fn from (x: %s) -> Self {@;<1 2> @[<v>" rs_name;
      fprintf f "match x {@;<1 2> @[<v>";
      pp_print_list
        (fun f { c_name; args; _ } ->
          let args =
            match args with
            | [] -> ""
            | _ ->
                let inner =
                  List.mapi args ~f:(fun i _ -> Printf.sprintf "x%d" i)
                  |> String.concat ~sep:", "
                in
                Printf.sprintf "(%s)" inner
          in
          fprintf f "%s::%s%s => %s::%s%s," rs_name (Id.pascal c_name) args
            (Id.pascal v_name) (Id.pascal c_name) args)
        f ctors;
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}@;@;";

      (* c -> rust *)
      fprintf f "impl From<%s> for %s {@;<1 2> @[<v>" (Id.pascal v_name) rs_name;
      fprintf f "fn from (x: %s) -> Self {@;<1 2> @[<v>" (Id.pascal v_name);
      fprintf f "match x {@;<1 2> @[<v>";
      pp_print_list
        (fun f { c_name; args; _ } ->
          let args =
            match args with
            | [] -> ""
            | _ ->
                let inner =
                  List.mapi args ~f:(fun i _ -> Printf.sprintf "x%d" i)
                  |> String.concat ~sep:", "
                in
                Printf.sprintf "(%s)" inner
          in
          fprintf f "%s::%s%s => %s::%s%s," (Id.pascal v_name)
            (Id.pascal c_name) args rs_name (Id.pascal c_name) args;
          ())
        f ctors;
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}";
      fprintf f "@]@;<1 0>}@;@;";

      ()

let print f items =
  pp_open_vbox f 0;
  List.iter items ~f:(function
    | Ir.Struct ({ s_name; fields; _ } as struc) ->
        fprintf f "#[derive(Clone, Copy)]@;";
        fprintf f "#[repr(C)]@;";
        fprintf f "pub struct %s {@;<1 2>@[<v>" (Id.pascal s_name);
        List.iteri fields ~f:(fun i { f_name; f_type; _ } ->
            if i > 0 then pp_print_space f ();
            fprintf f "pub %s: " (Id.str f_name);
            print_type f f_type;
            fprintf f ",");
        fprintf f "@]@;<1 0>}@;@;";

        print_impl_from_struct f struc
    | Ir.Variant ({ v_name; ctors; _ } as variant) ->
        fprintf f "#[derive(Clone, Copy)]@;";
        fprintf f "#[repr(C)]@;";
        fprintf f "pub enum %s {@;<1 2>@[<v>" (Id.pascal v_name);
        List.iteri ctors ~f:(fun i { c_name; args } ->
            if i > 0 then pp_print_space f ();
            fprintf f "%s" (Id.pascal c_name);

            if not (List.is_empty args) then (
              fprintf f "(@[";
              List.iteri args ~f:(fun i typ ->
                  if i > 0 then fprintf f ",@ ";
                  fprintf f "%a" print_type typ);
              fprintf f "@])");
            fprintf f ",");
        fprintf f "@]@;<1 0>}@;@;";

        print_impl_from_variant f variant
    | Fn _ -> ());

  pp_close_box f ()
