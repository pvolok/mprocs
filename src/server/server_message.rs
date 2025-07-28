use crate::{
  app::{ClientHandle, ClientId},
  protocol::CltToSrv,
};

pub type ServerSender = tokio::sync::mpsc::UnboundedSender<ServerMessage>;

pub enum ServerMessage {
  ClientMessage { client_id: ClientId, msg: CltToSrv },
  ClientConnected { handle: ClientHandle },
  ClientDisconnected { client_id: ClientId },
}
