use std::path::Path;

use rquickjs::{Context, Module, Object, Persistent, Runtime};

pub struct JsVm {
  #[allow(dead_code)]
  runtime: Runtime,
  pub context: Context,
}

impl JsVm {
  pub fn new() -> anyhow::Result<Self> {
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;

    context.with(|ctx| -> anyhow::Result<_> {
      ctx.globals().set(
        "print",
        rquickjs::function::Func::new(|arg: rquickjs::Value| {
          println!("{:?}", arg.as_value());
        }),
      )?;
      Ok(())
    })?;

    Ok(JsVm { runtime, context })
  }

  pub fn eval_file(
    &self,
    path: &Path,
    src: &[u8],
  ) -> anyhow::Result<Persistent<Object<'static>>> {
    let module = self.context.with(|ctx| -> anyhow::Result<_> {
      let name = if let Some(name) = path.file_name() {
        name.to_string_lossy()
      } else {
        path.to_string_lossy()
      };
      let module = Module::declare(ctx.clone(), name.as_bytes(), src)?;
      let (module, _promise) = module.eval()?;
      let module = Persistent::save(&ctx, module.namespace()?);
      Ok(module)
    })?;
    Ok(module)
  }
}
