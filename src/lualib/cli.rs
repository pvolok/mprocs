pub fn init_cli_lib(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
  let lib = lua.create_table()?;

  Ok(lib)
}
