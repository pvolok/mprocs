use rquickjs::{function::Opt, Ctx, Exception, Object};

use crate::js::rquickjs_ext::ObjectExt;

async fn exec_fn(
  ctx: Ctx<'_>,
  cmd: String,
  Opt(args): Opt<Vec<String>>,
) -> rquickjs::Result<Object<'_>> {
  let output = tokio::process::Command::new(&cmd)
    .args(args.unwrap_or_default())
    .output()
    .await
    .map_err(|e| {
      Exception::throw_message(&ctx, &format!("process.exec: {e}"))
    })?;
  let result = Object::new(ctx.clone())?;
  result.set(
    "stdout",
    String::from_utf8_lossy(&output.stdout).into_owned(),
  )?;
  result.set(
    "stderr",
    String::from_utf8_lossy(&output.stderr).into_owned(),
  )?;
  result.set("code", output.status.code().unwrap_or(-1))?;
  Ok(result)
}

pub fn init(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let obj = Object::new(ctx.clone())?;

  obj.def_fn_async("exec", exec_fn)?;

  obj.def_fn("cwd", || -> String {
    std::env::current_dir()
      .map(|p| p.to_string_lossy().to_string())
      .unwrap_or_default()
  })?;

  obj.def_fn("exit", |Opt(code): Opt<i32>| -> () {
    std::process::exit(code.unwrap_or(0))
  })?;

  obj.set("argv", std::env::args().collect::<Vec<String>>())?;
  obj.set("pid", std::process::id())?;

  Ok(obj)
}
