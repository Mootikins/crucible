//! Daemon-provided workflow step handlers.
//!
//! The engine in `crucible-core::workflow` ships a placeholder
//! [`DefaultHandler`][crucible_core::workflow::DefaultHandler] for dry
//! runs and engine-only tests. Handlers defined here depend on runtime
//! daemon state (agent manager, event bus, storage) and get plugged
//! into the dispatch table by
//! [`crate::rpc::workflow_handlers::handle_workflow_start`] when a
//! workflow is actually executed against a session.

mod inline;
pub mod interpolate;

pub use inline::DaemonInlineHandler;
