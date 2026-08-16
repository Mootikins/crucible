//! RPC dispatch layer
//!
//! Separates transport (Unix socket I/O) from dispatch logic, enabling
//! unit tests without spinning up a full server.

mod context;
mod dispatch;
mod params;
pub(crate) mod ui;
mod workflow_handlers;

pub use context::{DeferredShutdown, RpcContext};
#[allow(unused_imports)]
pub use dispatch::{RpcDispatcher, METHODS};
#[allow(unused_imports)]
pub use params::parse_params;
