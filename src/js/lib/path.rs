use std::path::{Path, PathBuf};

use rquickjs::{function::Rest, Ctx, Object};

use crate::js::rquickjs_ext::ObjectExt;

pub fn init(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx.clone())?;

  obj.def_fn("join", |Rest(parts): Rest<String>| -> String {
    let mut p = PathBuf::new();
    for part in parts {
      p.push(part);
    }
    p.to_string_lossy().to_string()
  })?;

  obj.def_fn("dirname", |path: String| -> String {
    Path::new(&path)
      .parent()
      .map(|p| p.to_string_lossy().to_string())
      .unwrap_or_default()
  })?;

  obj.def_fn("basename", |path: String| -> String {
    Path::new(&path)
      .file_name()
      .map(|n| n.to_string_lossy().to_string())
      .unwrap_or_default()
  })?;

  obj.def_fn("extname", |path: String| -> String {
    Path::new(&path)
      .extension()
      .map(|e| format!(".{}", e.to_string_lossy()))
      .unwrap_or_default()
  })?;

  Ok(obj)
}
