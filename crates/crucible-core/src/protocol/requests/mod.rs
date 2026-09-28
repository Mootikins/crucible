//! The request and reply types of the daemon RPC methods.
//!
//! Both sides of the wire use them: a client serializes each type, and the
//! daemon handler deserializes the same type. They live in core so that no
//! client needs the daemon crate to name them.

mod agent;
mod common;
mod lua;
mod notifications;
mod plugin;
mod proposals;
mod session;
mod storage;
mod subscription;
mod workflow;

pub use agent::*;
pub use common::*;
pub use lua::*;
pub use notifications::*;
pub use plugin::*;
pub use proposals::*;
pub use session::*;
pub use storage::*;
pub use subscription::*;
pub use workflow::*;
