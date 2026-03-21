use rquickjs::{
  function::IntoJsFunc,
  prelude::{Async, Func},
  Object,
};

pub(crate) trait ObjectExt<'js> {
  fn def_fn<F, A>(&self, name: &str, f: F) -> rquickjs::Result<()>
  where
    F: IntoJsFunc<'js, A> + 'js;

  fn def_fn_async<F, A>(&self, name: &str, f: F) -> rquickjs::Result<()>
  where
    Async<F>: IntoJsFunc<'js, A> + 'js;
}

impl<'js> ObjectExt<'js> for Object<'js> {
  fn def_fn<F, A>(&self, name: &str, f: F) -> rquickjs::Result<()>
  where
    F: IntoJsFunc<'js, A> + 'js,
  {
    self.set(name, Func::new(f))
  }

  fn def_fn_async<F, A>(&self, name: &str, f: F) -> rquickjs::Result<()>
  where
    Async<F>: IntoJsFunc<'js, A> + 'js,
  {
    self.set(name, Func::new(Async(f)))
  }
}
