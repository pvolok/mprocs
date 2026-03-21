mod env;
mod fs;
mod log;
mod path;
mod process;

use rquickjs::{function::This, object::Accessor, Ctx, Object};

pub fn init(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
  let dk = Object::new(ctx.clone())?;

  log::define(&dk)?;

  register_lazy(&dk, "fs", fs::init)?;
  register_lazy(&dk, "path", path::init)?;
  register_lazy(&dk, "env", env::init)?;
  register_lazy(&dk, "process", process::init)?;

  ctx.globals().set("dk", dk)?;
  Ok(())
}

pub(crate) fn register_lazy<'js>(
  obj: &Object<'js>,
  name: &str,
  factory: fn(Ctx<'js>) -> rquickjs::Result<Object<'js>>,
) -> rquickjs::Result<()> {
  let name_owned = name.to_string();
  obj.prop(
    name,
    Accessor::from(
      move |this: This<Object<'js>>,
            ctx: Ctx<'js>|
            -> rquickjs::Result<Object<'js>> {
        let obj = factory(ctx)?;
        this.0.prop(&*name_owned, obj.clone())?;
        Ok(obj)
      },
    )
    .configurable()
    .enumerable(),
  )?;
  Ok(())
}
