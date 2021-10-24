open Base

module Id = struct
  type t = { default : string }

  let make default = { default }

  let str id = id.default

  let pascal id =
    let words = String.split ~on:'_' id.default in
    let words = List.map words ~f:String.capitalize in
    String.concat words
end
type id = Id.t
let id = Id.make

module Type = struct
  type t = {
    c : string;
    ml : string;
    ml_c : string;
    ml_of_c : string;
    ml_to_c : string;
    rs : string;
    rs_of_c : string -> string;
    rs_to_c : string -> string;
  }

  let make ~c ~ml ?ml_c ~ml_of_c ~ml_to_c ~rs ?rs_of_c ?rs_to_c () =
    {
      c;
      ml;
      ml_c = Option.value ml_c ~default:c;
      ml_of_c;
      ml_to_c;
      rs;
      rs_of_c =
        Option.value rs_of_c ~default:(fun var ->
            Printf.sprintf "%s.into()" var);
      rs_to_c =
        Option.value rs_to_c ~default:(fun var ->
            Printf.sprintf "%s.into()" var);
    }

  let c t = t.c
  let ml t = t.ml
  let ml_c t = t.ml_c
  let rs t = t.rs

  let ml_of_c t = t.ml_of_c
  let ml_to_c t = t.ml_to_c

  let rs_of_c t = t.rs_of_c
  let rs_to_c t = t.rs_to_c
end
type typ = Type.t

and decl =
  | Struct of struc
  | Variant of variant
  | Fn of fn

and struc = {
  s_name : id;
  fields : field list;
  s_rs_name : string option;
}

and field = {
  f_name : id;
  f_type : typ;
  f_rs_name : string option;
}

and variant = {
  v_name : id;
  ctors : ctor list;
  v_rs_name : string option;
}

and ctor = {
  c_name : id;
  args : typ list;
}

and fn = {
  fn_name : string;
  fn_args : fn_arg list;
  fn_ret : typ;
}

and fn_arg = { fa_type : typ }

let uint8 =
  Type.make ~c:"uint8_t" ~ml:"int" ~ml_of_c:"Unsigned.UInt8.to_int"
    ~ml_to_c:"Unsigned.UInt8.of_int" ~rs:"u8" ()
let uint16 =
  Type.make ~c:"uint16_t" ~ml:"int" ~ml_of_c:"Unsigned.UInt16.to_int"
    ~ml_to_c:"Unsigned.UInt16.of_int" ~rs:"u16" ()
let uint32 =
  Type.make ~c:"uint32_t" ~ml:"int" ~ml_of_c:"Unsigned.UInt32.to_int"
    ~ml_to_c:"Unsigned.UInt32.of_int" ~rs:"u32" ()

let struc ?rs_name name fields =
  Struct { s_name = name; fields; s_rs_name = rs_name }
let field ?rs_name name typ =
  { f_name = name; f_type = typ; f_rs_name = rs_name }

let variant ?rs_name name ctors =
  Variant { v_name = name; ctors; v_rs_name = rs_name }
let ctor name args = { c_name = name; args }

let fn name args ret = { fn_name = name; fn_args = args; fn_ret = ret }
