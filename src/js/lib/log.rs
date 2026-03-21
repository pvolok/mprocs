use rquickjs::{function::Rest, Object};

use crate::js::rquickjs_ext::ObjectExt;

pub fn define(dk: &Object<'_>) -> rquickjs::Result<()> {
  dk.def_fn("log", |Rest(args): Rest<String>| {
    eprintln!("{}", args.join(" "));
  })?;
  dk.def_fn("warn", |Rest(args): Rest<String>| {
    eprintln!("{}", args.join(" "));
  })?;
  dk.def_fn("error", |Rest(args): Rest<String>| {
    eprintln!("{}", args.join(" "));
  })?;
  Ok(())
}
