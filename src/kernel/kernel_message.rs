use crate::{
  app::{ClientHandle, ClientId},
  protocol::CltToSrv,
};

pub type KernelSender = tokio::sync::mpsc::UnboundedSender<KernelMessage>;

pub enum KernelMessage {
  ClientMessage { client_id: ClientId, msg: CltToSrv },
  ClientConnected { handle: ClientHandle },
  ClientDisconnected { client_id: ClientId },
}
