use mlua::IntoLua as _;

mod cli;
mod dbg;
mod fs;
mod os;

pub fn init_std(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
  let std = lua.create_table()?;

  let std_meta = lua.create_table()?;
  std_meta.set(
    mlua::MetaMethod::Index.name(),
    lua.create_function(|lua, (this, key): (mlua::Table, mlua::String)| {
      let value = match key.to_str()?.as_ref() {
        "cli" => self::cli::init_cli_lib(lua)?,
        "dbg" => self::dbg::init_dbg_lib(lua)?,
        "fs" => self::fs::init_fs_lib(lua)?,
        "os" => self::os::init_os_lib(lua)?,
        _ => return Ok(mlua::Value::Nil),
      };
      this.raw_set(key, value.clone())?;
      value.into_lua(lua)
    })?,
  )?;

  std.set_metatable(Some(std_meta))?;

  Ok(std)
}
