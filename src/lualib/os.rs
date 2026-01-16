use std::{process::Stdio, time::Duration};

use futures::FutureExt;
use mlua::IntoLua;
use serde::ser::Error;
use tokio_util::time::FutureExt as _;

pub struct ChildProcess {
  result_receiver: Option<
    futures::future::Shared<
      tokio::sync::oneshot::Receiver<std::process::Output>,
    >,
  >,
  // exit_sender: tokio::sync::mpsc::UnboundedSender<()>,
}

impl ChildProcess {
  pub fn spawn(mut cmd: tokio::process::Command) -> std::io::Result<Self> {
    let mut child = cmd.spawn()?;
    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let res_rx = res_rx.shared();
    let (_f_tx, mut f_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    tokio::spawn(async move {
      loop {
        tokio::select! {
          _ = child.wait() => {
            match child.wait_with_output().await {
              Ok(output) => {
                let _: Result<_, _> = res_tx.send(output);
                break;
              }
              Err(_err) => {
                break;
              }
            }
          }
          _ = f_rx.recv() => {
            let _ : Result<_, _> = child.start_kill();
          }
        }
      }
    });
    Ok(Self {
      result_receiver: Some(res_rx),
      // exit_sender: f_tx,
    })
  }
}

impl mlua::UserData for ChildProcess {
  fn add_fields<F: mlua::UserDataFields<Self>>(_fields: &mut F) {}

  fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
    methods.add_async_function("wait", async |lua, this: mlua::AnyUserData| {
      let this_ = this.clone().borrow_mut::<ChildProcess>()?;
      if let Some(output) = this_.result_receiver.clone() {
        let output = output.clone().await.map_err(mlua::Error::external)?;
        let output_tbl = lua.create_table()?;
        output_tbl.set("status", output.status.code())?;
        output_tbl.set("stdout", mlua::String::wrap(output.stdout))?;
        output_tbl.set("stderr", mlua::String::wrap(output.stderr))?;
        return output_tbl.into_lua(&lua);
      }
      Ok(mlua::Value::Nil)
    });
  }
}

pub fn init_os_lib(lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
  let lib = lua.create_table()?;

  lib.set(
    "spawn",
    lua.create_function(|_lua, opts: mlua::Value| {
      let cmd = match opts {
        mlua::Value::Table(table) => {
          let mut args_iter = table.sequence_values::<String>();
          let program = args_iter.next().ok_or_else(|| {
            mlua::Error::external("At least one arg is expected.")
          })??;
          let args = args_iter.collect::<Result<Vec<_>, _>>()?;
          let mut cmd = tokio::process::Command::new(program);
          cmd.args(args);
          cmd.stdout(Stdio::piped());
          cmd.stderr(Stdio::piped());
          cmd
        }
        _ => return Err(mlua::Error::custom("Expected a list of arguments.")),
      };
      Ok(ChildProcess::spawn(cmd)?)
    })?,
  )?;

  lib.set(
    "spawn_async",
    lua.create_async_function(async |lua, a: mlua::Function| {
      let token = lua
        .app_data_ref::<tokio_util::sync::CancellationToken>()
        .unwrap()
        .clone();
      tokio::spawn(async move {
        a.call_async::<mlua::Value>(())
          .with_cancellation_token_owned(token)
          .await;
      });
      Ok(())
    })?,
  )?;
  lib.set(
    "select",
    lua.create_async_function(async |_lua, items: mlua::Table| {
      let mut tasks: Vec<mlua::AsyncThread<mlua::Value>> = Vec::new();
      for thread in items.sequence_values::<mlua::Thread>() {
        let thread = thread?;
        tasks.push(thread.into_async(())?);
      }
      let (result, _index, _tasks) = futures::future::select_all(tasks).await;
      result
    })?,
  )?;
  lib.set(
    "both",
    lua.create_async_function(
      async |_lua, (a, b): (mlua::Function, mlua::Function)| {
        let mut ls = tokio::task::JoinSet::new();
        let _a = ls.spawn(a.call_async::<()>(()));
        let _b = ls.spawn(b.call_async::<()>(()));
        while (ls.join_next().await).is_some() {}
        Ok(())
      },
    )?,
  )?;
  lib.set(
    "wait_echo",
    lua.create_async_function(async |_lua, a: mlua::String| {
      tokio::time::sleep(Duration::from_secs(1)).await;
      println!("- {:?}", a);
      Ok(())
    })?,
  )?;

  Ok(lib)
}
