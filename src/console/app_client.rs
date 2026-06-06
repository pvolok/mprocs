use std::fmt::Debug;

use crate::{
  console::server_message::ServerMessage,
  ipc::{receiver::MsgReceiver, sender::MsgSender},
  kernel::{kernel_message::TaskSender, task::TaskCmd},
  protocol::{ClientId, CltToSrv, SrvToClt},
  term::{ScreenDiffer, Size},
};

pub async fn client_loop(
  id: ClientId,
  app_sender: TaskSender,
  (client_sender, mut server_receiver): (
    MsgSender<SrvToClt>,
    MsgReceiver<CltToSrv>,
  ),
) {
  log::debug!("client_loop: server_receiver.recv()");
  let init_msg = server_receiver.recv().await;
  let size = match init_msg {
    Some(Ok(CltToSrv::Init { width, height })) => Size { width, height },
    Some(Ok(msg)) => {
      log::warn!("client_loop: expected init message, got {msg:?}");
      return;
    }
    Some(Err(err)) => {
      log::warn!("client_loop: failed to decode init message: {err}");
      return;
    }
    None => return,
  };
  client_session(id, app_sender, size, client_sender, server_receiver).await;
}

pub async fn client_session(
  id: ClientId,
  app_sender: TaskSender,
  size: Size,
  client_sender: MsgSender<SrvToClt>,
  mut server_receiver: MsgReceiver<CltToSrv>,
) {
  match ClientHandle::create(id, client_sender, size) {
    Ok(handle) => {
      app_sender.send(TaskCmd::msg(ServerMessage::ClientConnected { handle }));
    }
    Err(err) => {
      log::error!("Client creation error: {:?}", err);
      return;
    }
  }

  loop {
    let msg = if let Some(msg) = server_receiver.recv().await {
      msg
    } else {
      break;
    };

    match msg {
      Ok(msg) => {
        app_sender.send(TaskCmd::msg(ServerMessage::ClientMessage {
          client_id: id,
          msg,
        }));
      }
      Err(_err) => break,
    }
  }
  app_sender.send(TaskCmd::msg(ServerMessage::ClientDisconnected {
    client_id: id,
  }));
}

pub struct ClientHandle {
  pub id: ClientId,
  pub sender: MsgSender<SrvToClt>,
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
  pub fn create(
    id: ClientId,
    client_sender: MsgSender<SrvToClt>,
    size: Size,
  ) -> anyhow::Result<Self> {
    Ok(Self {
      id,
      sender: client_sender,
      screen_size: size,
      differ: ScreenDiffer::new(),
    })
  }

  pub fn size(&self) -> Size {
    self.screen_size
  }

  pub fn resize(&mut self, size: Size) {
    self.screen_size = size;
  }
}
