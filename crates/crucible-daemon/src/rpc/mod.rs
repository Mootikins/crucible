//! RPC dispatch layer
//!
//! Separates transport (Unix socket I/O) from dispatch logic, enabling
//! unit tests without spinning up a full server.

mod context;
mod dispatch;
mod knob_method;
#[cfg(test)]
mod missing_session_contract;
mod params;
pub(crate) mod ui;
mod workflow_handlers;

pub use context::{DeferredShutdown, RpcContext, RpcContextParams};
#[allow(unused_imports)]
pub use dispatch::{
    ConfigOriginRow, ConfigSaveReply, RpcDispatcher, RpcMethod, WebhookReceiveReply, METHODS,
};
#[allow(unused_imports)]
pub use knob_method::rpc_set_method;
#[allow(unused_imports)]
pub use params::parse_params;
