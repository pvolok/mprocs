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

  obj.def_fn("resolve", |Rest(parts): Rest<String>| -> String {
    let mut p = std::env::current_dir().unwrap_or_default();
    for part in parts {
      let path = Path::new(&part);
      if path.is_absolute() {
        p = path.to_path_buf();
      } else {
        p.push(path);
      }
    }
    p.to_string_lossy().to_string()
  })?;

  obj.def_fn("isAbsolute", |path: String| -> bool {
    Path::new(&path).is_absolute()
  })?;

  Ok(obj)
}
