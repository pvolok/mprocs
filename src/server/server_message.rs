use crate::{
  app::{ClientHandle, ClientId},
  proc::msg::CustomProcCmd,
  protocol::CltToSrv,
};

#[derive(Debug)]
pub enum ServerMessage {
  ClientMessage { client_id: ClientId, msg: CltToSrv },
  ClientConnected { handle: ClientHandle },
  ClientDisconnected { client_id: ClientId },
}

impl CustomProcCmd for ServerMessage {}
