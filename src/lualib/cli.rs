use std::{str::FromStr as _, sync::OnceLock};

use mlua::LuaSerdeExt;
use tui::{layout::Rect, style::Modifier, text::Span};

use crate::term::term_driver::TermDriver;

static DRIVER: OnceLock<tokio::sync::Mutex<Option<tui::Terminal<TermDriver>>>> =
  OnceLock::new();

fn get_global_driver(
) -> &'static tokio::sync::Mutex<Option<tui::Terminal<TermDriver>>> {
  let driver = DRIVER.get_or_init(|| tokio::sync::Mutex::new(None));
  driver
}

pub fn init_cli_lib(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
  let lib = lua.create_table()?;

  lib.set(
    "input",
    lua.create_async_function(async |lua, ()| {
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      let term = driver_holder.get_or_insert_with(|| {
        tui::Terminal::new(TermDriver::create().unwrap()).unwrap()
      });
      let event = term.backend_mut().input().await?;
      let event = lua.to_value(&event)?;
      Ok(event)
    })?,
  )?;

  lib.set(
    "screen_size",
    lua.create_function(|lua, ()| {
      let (x, y) = crossterm::terminal::size()?;
      let size = lua.create_table_from([("x", x), ("y", y)])?;
      Ok(size)
    })?,
  )?;

  lib.set(
    "enter",
    lua.create_function(|_lua, ()| {
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      driver_holder.get_or_insert_with(|| {
        tui::Terminal::new(TermDriver::create().unwrap()).unwrap()
      });
      Ok(())
    })?,
  )?;
  lib.set(
    "exit",
    lua.create_function(|_lua, ()| {
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      if let Some(mut driver) = driver_holder.take() {
        driver
          .backend_mut()
          .destroy()
          .map_err(mlua::Error::external)?;
      }
      Ok(())
    })?,
  )?;

  lib.set(
    "block",
    lua.create_function(|lua, (_opts, area): (mlua::Table, mlua::Value)| {
      let area = lua.from_value::<Rect>(area)?;
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      let term = driver_holder.as_mut().unwrap();
      term.get_frame().render_widget(
        tui::widgets::Block::bordered()
          .border_type(tui::widgets::BorderType::Rounded),
        area,
      );
      Ok(())
    })?,
  )?;
  lib.set(
    "text",
    lua.create_function(|lua, (text, area): (mlua::Value, mlua::Value)| {
      let mut spans = Vec::new();
      fn collect_spans(
        acc: &mut Vec<Span>,
        item: mlua::Value,
        style: tui::style::Style,
      ) -> mlua::Result<()> {
        match item {
          mlua::Value::Nil => (),
          mlua::Value::Boolean(v) => {
            if v {
              acc.push(tui::text::Span::styled("true", style));
            }
          }
          mlua::Value::Integer(v) => {
            acc.push(tui::text::Span::styled(v.to_string(), style));
          }
          mlua::Value::Number(v) => {
            acc.push(tui::text::Span::styled(v.to_string(), style));
          }
          mlua::Value::String(v) => {
            acc.push(tui::text::Span::styled(v.display().to_string(), style));
          }
          mlua::Value::Table(item) => {
            let mut style = style;
            if let mlua::Value::String(fg) = item.get("fg")? {
              let color = tui::style::Color::from_str(fg.to_str()?.as_ref())
                .map_err(mlua::Error::external)?;
              style = style.fg(color);
            }
            if let mlua::Value::String(bg) = item.get("bg")? {
              let color = tui::style::Color::from_str(bg.to_str()?.as_ref())
                .map_err(mlua::Error::external)?;
              style = style.bg(color);
            }
            if let mlua::Value::Boolean(true) =
              item.get::<mlua::Value>("bold")?
            {
              style = style.add_modifier(Modifier::BOLD);
            }
            if let mlua::Value::Boolean(true) =
              item.get::<mlua::Value>("italic")?
            {
              style = style.add_modifier(Modifier::ITALIC);
            }

            for item in item.sequence_values::<mlua::Value>() {
              let item = item?;
              collect_spans(acc, item, style)?;
            }
          }
          item => {
            acc.push(tui::text::Span::styled(
              format!("<{}>", item.type_name()),
              tui::style::Style::new()
                .fg(tui::style::Color::White)
                .bg(tui::style::Color::Red),
            ));
          }
        }
        Ok(())
      }
      collect_spans(&mut spans, text, tui::style::Style::new())?;

      let area = lua.from_value::<Rect>(area)?;
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      let term = driver_holder.as_mut().unwrap();
      term.get_frame().render_widget(
        tui::widgets::Paragraph::new(tui::text::Line::from(spans))
          .wrap(tui::widgets::Wrap { trim: false }),
        area,
      );
      Ok(())
    })?,
  )?;

  lib.set(
    "begin_frame",
    lua.create_function(|_lua, ()| {
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      let term = driver_holder.as_mut().unwrap();
      term.autoresize()?;
      Ok(())
    })?,
  )?;

  lib.set(
    "end_frame",
    lua.create_function(|_lua, ()| {
      let mut driver_holder = get_global_driver()
        .try_lock()
        .map_err(mlua::Error::external)?;
      let term = driver_holder.as_mut().unwrap();
      let _: Result<_, _> = term.draw(|_f| {});
      Ok(())
    })?,
  )?;

  Ok(lib)
}
