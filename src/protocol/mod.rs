pub mod attach;
pub mod rpc;

pub use attach::{ClientId, CltToSrv, SrvToClt};
pub use rpc::{DkRequest, DkResponse, DkTaskInfo};
