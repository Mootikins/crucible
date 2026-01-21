//! RPC dispatch layer
//!
//! Separates transport (Unix socket I/O) from dispatch logic, enabling
//! unit tests without spinning up a full server.

mod context;
mod dispatch;
mod error;
mod params;

pub use context::RpcContext;
pub use dispatch::{RpcDispatcher, METHODS};
pub use error::{RpcResult, ToRpcError};
pub use params::parse_params;
