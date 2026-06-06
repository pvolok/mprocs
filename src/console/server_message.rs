use crate::{
  console::app_client::ClientHandle,
  protocol::{ClientId, CltToSrv},
};

#[derive(Debug)]
pub enum ServerMessage {
  ClientMessage { client_id: ClientId, msg: CltToSrv },
  ClientConnected { handle: ClientHandle },
  ClientDisconnected { client_id: ClientId },
}
