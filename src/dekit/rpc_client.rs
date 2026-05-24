use std::path::Path;

use crate::daemon::socket::connect_client_socket;
use crate::protocol::{CltToSrv, DkRequest, DkResponse, SrvToClt};

pub async fn rpc_request(
  working_dir: &Path,
  req: DkRequest,
  spawn_server: bool,
) -> anyhow::Result<DkResponse> {
  let (mut sender, mut receiver) =
    connect_client_socket::<CltToSrv, SrvToClt>(working_dir, spawn_server)
      .await?;
  sender.send(CltToSrv::Rpc(req)).await?;
  match receiver.recv().await {
    Some(Ok(SrvToClt::Rpc(resp))) => Ok(resp),
    Some(Ok(other)) => {
      anyhow::bail!("unexpected response: {:?}", other)
    }
    Some(Err(e)) => anyhow::bail!("decode error: {}", e),
    None => anyhow::bail!("connection closed without response"),
  }
}
