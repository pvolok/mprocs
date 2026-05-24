use serde::{Deserialize, Serialize};

use crate::protocol::rpc::{DkRequest, DkResponse};
use crate::term::TermEvent;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientId(pub u32);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SrvToClt {
  Print(String),
  Flush,
  Quit,
  Rpc(DkResponse),
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum CltToSrv {
  Init { width: u16, height: u16 },
  Key(TermEvent),
  Rpc(DkRequest),
}
