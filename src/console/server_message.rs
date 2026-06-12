use serde::{Deserialize, Serialize};

use crate::{console::app_client::ClientHandle, term::TermEvent};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientId(pub u32);

#[derive(Debug)]
pub enum ServerMessage {
  ClientInput {
    client_id: ClientId,
    event: TermEvent,
  },
  ClientConnected {
    handle: ClientHandle,
  },
  ClientDisconnected {
    client_id: ClientId,
  },
}
