pub fn init_fs_lib(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
  let fs = lua.create_table()?;

  fs.set(
    "read",
    lua.create_function(|lua, path: mlua::String| {
      lua.create_thread(
        lua
          .create_async_function(async move |lua, path: mlua::String| {
            let bytes = tokio::fs::read(path.to_str()?.as_ref()).await?;
            Ok(mlua::String::wrap(bytes))
          })?
          .bind(path)?,
      )
    })?,
  )?;

  Ok(fs)
}
