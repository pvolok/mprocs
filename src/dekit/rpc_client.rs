use std::path::Path;

use anyhow::bail;
use serde_json::Value;

use crate::daemon::socket::connect_client_socket;
use crate::protocol::{CtlMsg, DkRequest, Request, client_handshake};

pub async fn rpc_request(
  working_dir: &Path,
  req: DkRequest,
  spawn_server: bool,
) -> anyhow::Result<Value> {
  let (mut sender, mut receiver) =
    connect_client_socket(working_dir, spawn_server).await?;
  client_handshake(&mut sender, &mut receiver).await?;

  let (method, params) = req.to_wire();
  sender
    .send_ctl(CtlMsg::Request(Request {
      id: 1,
      method,
      params,
    }))
    .await?;

  loop {
    match receiver.recv_ctl().await? {
      CtlMsg::Response(response) => {
        if response.id != 1 {
          continue;
        }
        match response.error {
          Some(error) => bail!("{error}"),
          None => return Ok(response.result.unwrap_or(Value::Null)),
        }
      }
      CtlMsg::Bye(bye) => bail!("daemon closed the connection: {}", bye.code),
      msg @ (CtlMsg::Hello(_) | CtlMsg::Request(_) | CtlMsg::Event(_)) => {
        log::debug!("ignoring daemon message {msg:?}");
      }
    }
  }
}
