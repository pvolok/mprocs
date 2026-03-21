use std::path::Path;

use rquickjs::{AsyncContext, AsyncRuntime, Ctx, Module, Object, Persistent};

pub struct JsVm {
  #[allow(dead_code)]
  runtime: AsyncRuntime,
  pub context: AsyncContext,
}

impl JsVm {
  pub async fn new() -> anyhow::Result<Self> {
    let runtime = AsyncRuntime::new()?;
    let context = AsyncContext::full(&runtime).await?;

    rquickjs::async_with!(context => |ctx| {
      super::lib::init(&ctx)
    })
    .await?;

    Ok(JsVm { runtime, context })
  }

  pub async fn eval_file(
    &self,
    path: &Path,
    src: &[u8],
  ) -> anyhow::Result<Persistent<Object<'static>>> {
    let src = src.to_vec();
    let path = path.to_path_buf();
    let module = rquickjs::async_with!(self.context => |ctx| {
      eval_module(&ctx, &path, src)
    })
    .await?;
    Ok(module)
  }
}

fn eval_module(
  ctx: &Ctx<'_>,
  path: &Path,
  src: Vec<u8>,
) -> rquickjs::Result<Persistent<Object<'static>>> {
  let name = if let Some(name) = path.file_name() {
    name.to_string_lossy()
  } else {
    path.to_string_lossy()
  };
  let module = Module::declare(ctx.clone(), name.as_bytes(), src)?;
  let (module, _promise) = module.eval()?;
  Ok(Persistent::save(ctx, module.namespace()?))
}
