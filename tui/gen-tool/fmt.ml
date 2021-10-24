open Base
open Caml.Format

let wrap_list ?pp_sep f open_ close items func =
  fprintf f "%s@;<1 2>@[<v>" open_;
  pp_print_list ?pp_sep func f items;
  fprintf f "@]@;<1 0>%s" close

let wrap_block f open_ close func =
  fprintf f "%s@;<1 2>@[<v>" open_;
  func ();
  fprintf f "@]@;<1 0>%s" close
