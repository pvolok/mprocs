use rquickjs::{Object, function::Rest};

use crate::js::rquickjs_ext::ObjectExt;

pub fn define(obj: &Object<'_>) -> rquickjs::Result<()> {
  obj.def_fn("log", |Rest(args): Rest<String>| {
    eprintln!("{}", args.join(" "));
  })?;
  obj.def_fn("warn", |Rest(args): Rest<String>| {
    eprintln!("{}", args.join(" "));
  })?;
  obj.def_fn("error", |Rest(args): Rest<String>| {
    eprintln!("{}", args.join(" "));
  })?;
  Ok(())
}
