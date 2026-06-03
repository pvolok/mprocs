use std::fmt::Debug;

use crate::{
  ipc::{receiver::MsgReceiver, sender::MsgSender},
  kernel::{kernel_message::TaskSender, task::TaskCmd},
  mprocs::server_message::ServerMessage,
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
  match init_msg {
    Some(Ok(CltToSrv::Init { width, height })) => {
      let client_handle =
        ClientHandle::create(id, client_sender, Size { width, height });
      match client_handle {
        Ok(handle) => {
          app_sender
            .send(TaskCmd::msg(ServerMessage::ClientConnected { handle }));
        }
        Err(err) => {
          log::error!("Client creation error: {:?}", err);
        }
      }
    }
    Some(Ok(msg)) => {
      log::warn!("client_loop: expected init message, got {msg:?}");
      return;
    }
    Some(Err(err)) => {
      log::warn!("client_loop: failed to decode init message: {err}");
      return;
    }
    None => return,
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
