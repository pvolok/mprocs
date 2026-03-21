use rquickjs::{Ctx, Exception, Object};

use crate::js::rquickjs_ext::ObjectExt;

pub fn init(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx.clone())?;

  obj.def_fn_async("read", async move |ctx: Ctx<'_>, path: String| {
    tokio::fs::read_to_string(&path)
      .await
      .map_err(|e| Exception::throw_message(&ctx, &format!("fs.read: {e}")))
  })?;

  obj.def_fn_async(
    "write",
    async move |ctx: Ctx<'_>, path: String, content: String| {
      tokio::fs::write(&path, content)
        .await
        .map_err(|e| Exception::throw_message(&ctx, &format!("fs.write: {e}")))
    },
  )?;

  obj.def_fn_async("exists", async |path: String| {
    tokio::fs::metadata(&path).await.is_ok()
  })?;

  Ok(obj)
}
