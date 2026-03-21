use rquickjs::{Ctx, Object};

use crate::js::rquickjs_ext::ObjectExt;

pub fn init(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx.clone())?;

  obj.def_fn("get", |key: String| -> Option<String> {
    std::env::var(&key).ok()
  })?;

  Ok(obj)
}
