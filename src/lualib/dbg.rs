use std::collections::HashSet;

use mlua::{MetaMethod, Value};

pub fn init_dbg_lib(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
  let lib = lua.create_table()?;

  let mt = lua.create_table()?;
  mt.set(
    MetaMethod::Call.name(),
    lua.create_function(|_lua, (_this, value): (mlua::Value, _)| {
      println!("{}", inspect(value)?);
      Ok(())
    })?,
  )?;

  lib.set_metatable(Some(mt))?;

  lib.set(
    "inspect",
    lua.create_function(|_lua, value: mlua::Value| inspect(value))?,
  )?;

  Ok(lib)
}

pub fn inspect(obj: Value) -> mlua::Result<String> {
  let mut buf = String::new();
  collect(&mut buf, &mut HashSet::new(), 0, obj)?;
  Ok(buf)
}

fn indent(buf: &mut String, lvl: u8) {
  for _ in 0..lvl {
    buf.push_str("  ");
  }
}

fn collect(
  buf: &mut String,
  rec: &mut HashSet<usize>,
  lvl: u8,
  obj: Value,
) -> mlua::Result<()> {
  match obj {
    Value::Nil => buf.push_str("nil"),
    Value::Boolean(x) => buf.push_str(&x.to_string()),
    Value::LightUserData(_) => buf.push_str("<lightuserdata>"),
    Value::Integer(x) => buf.push_str(&x.to_string()),
    Value::Number(x) => buf.push_str(&x.to_string()),
    Value::String(x) => {
      buf.push('"');
      buf.push_str(&x.to_string_lossy().escape_debug().to_string());
      buf.push('"');
    }
    Value::Table(tbl) => {
      let ptr = tbl.to_pointer() as usize;
      if rec.contains(&ptr) {
        buf.push_str("<recursive>");
        return Ok(());
      } else {
        rec.insert(ptr);
      }

      let mut empty = true;
      buf.push('{');
      if let Some(mt) = tbl.metatable() {
        buf.push('\n');
        indent(buf, lvl + 1);
        buf.push_str("<metatable> = ");
        collect(buf, rec, lvl + 1, Value::Table(mt))?;

        empty = false;
      }
      for pair in tbl.pairs::<Value, Value>() {
        let (k, v) = pair?;
        buf.push('\n');
        indent(buf, lvl + 1);
        collect(buf, rec, lvl + 1, k)?;
        buf.push_str(" = ");
        collect(buf, rec, lvl + 1, v)?;

        empty = false;
      }
      if !empty {
        buf.push('\n');
        indent(buf, lvl);
      }
      buf.push('}');

      rec.remove(&ptr);
    }
    Value::Function(_) => buf.push_str("<fun>"),
    Value::Thread(_) => buf.push_str("<thread>"),
    Value::UserData(userdata) => {
      buf.push_str("<userdata>");
      if let Ok(mt) = userdata.metatable() {
        match mt.get(mlua::MetaMethod::ToString) {
          Ok(Value::Function(to_string)) => {
            match to_string.call::<Value>(userdata) {
              Ok(Value::String(str)) => {
                let str = str.to_string_lossy();
                buf.push_str(" \"");
                buf.push_str(&str.escape_debug().to_string());
                buf.push('"');
              }
              _ => (),
            }
          }
          _ => (),
        }
      }
    }
    Value::Other(_) => buf.push_str("<other>"),
    Value::Error(_) => buf.push_str("<error>"),
  }

  Ok(())
}
