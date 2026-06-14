use std::fmt::Debug;

use crate::{
  console::server_message::{ClientId, ServerMessage},
  kernel::kernel_message::TaskSender,
  protocol::{
    ConnReceiver, ConnSender, CtlMsg, Msg, RpcError, RpcRequest, codes,
    ctl::EVENT_INPUT, ok_result, server_handshake,
  },
  term::{ScreenDiffer, Size, TermEvent},
};

pub async fn client_loop(
  id: ClientId,
  app_sender: TaskSender,
  (mut sender, mut receiver): (ConnSender, ConnReceiver),
) {
  if let Err(err) = server_handshake(&mut sender, &mut receiver).await {
    log::warn!("client_loop: handshake failed: {err}");
    return;
  }
  let request = match receiver.recv_ctl().await {
    Ok(CtlMsg::Request(request)) => request,
    Ok(msg) => {
      log::warn!("client_loop: expected attach request, got {msg:?}");
      return;
    }
    Err(err) => {
      log::warn!("client_loop: {err}");
      return;
    }
  };
  match RpcRequest::from_wire(&request.method, request.params) {
    Ok(RpcRequest::Attach { width, height }) => {
      client_session(
        id,
        app_sender,
        Size { width, height },
        request.id,
        sender,
        receiver,
      )
      .await;
    }
    Ok(_) | Err(_) => {
      let error =
        RpcError::new(codes::UNKNOWN_METHOD, "only attach is supported here");
      let _ = sender.send_ctl(CtlMsg::err(request.id, error)).await;
    }
  }
}

pub async fn client_session(
  id: ClientId,
  app_sender: TaskSender,
  size: Size,
  request_id: u64,
  mut sender: ConnSender,
  mut receiver: ConnReceiver,
) {
  if let Err(err) = sender.send_ctl(CtlMsg::ok(request_id, ok_result())).await {
    log::warn!("client_session: failed to confirm attach: {err}");
    return;
  }

  app_sender.send(ServerMessage::ClientConnected {
    handle: ClientHandle {
      id,
      sender,
      screen_size: size,
      differ: ScreenDiffer::new(),
    },
  });

  loop {
    let msg = match receiver.recv().await {
      Some(Ok(msg)) => msg,
      Some(Err(err)) => {
        log::warn!("client_session: closing: {err}");
        break;
      }
      None => break,
    };
    match msg {
      Msg::Ctl(CtlMsg::Event(event)) => {
        if event.name != EVENT_INPUT {
          log::debug!("client_session: ignoring event '{}'", event.name);
          continue;
        }
        match serde_json::from_value::<TermEvent>(event.params) {
          Ok(event) => {
            app_sender.send(ServerMessage::ClientInput {
              client_id: id,
              event,
            });
          }
          Err(err) => {
            log::debug!("client_session: dropping input event: {err}");
          }
        }
      }
      Msg::Ctl(msg) => {
        log::debug!("client_session: ignoring message {msg:?}");
      }
      Msg::Out(_) => {
        log::debug!("client_session: ignoring output frame from client");
      }
    }
  }
  app_sender.send(ServerMessage::ClientDisconnected { client_id: id });
}

pub struct ClientHandle {
  pub id: ClientId,
  pub sender: ConnSender,
  pub screen_size: Size,
  pub differ: ScreenDiffer,
}

impl Debug for ClientHandle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ClientHandle")
      .field("id", &self.id)
      .finish()
  }
}

impl ClientHandle {
  pub fn size(&self) -> Size {
    self.screen_size
  }

  pub fn resize(&mut self, size: Size) {
    self.screen_size = size;
  }
}
