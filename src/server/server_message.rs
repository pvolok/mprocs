use crate::{
  mprocs::app::{ClientHandle, ClientId},
  protocol::CltToSrv,
};

#[derive(Debug)]
pub enum ServerMessage {
  ClientMessage { client_id: ClientId, msg: CltToSrv },
  ClientConnected { handle: ClientHandle },
  ClientDisconnected { client_id: ClientId },
}
