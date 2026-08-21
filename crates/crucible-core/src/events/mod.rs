//! Event system for Crucible.
//!
//! # What is here
//!
//! - [`SessionEvent`] — the canonical event type, shared by the daemon bus,
//!   the Lua bridge (`crucible-lua/src/handlers/conversion.rs`) and markdown
//!   rendering.
//! - `emitter` — the EventBus the **file-watch pipeline** runs on
//!   (`crucible-daemon/src/watch/`, `file_watch_bridge.rs`). Documented for
//!   years as "legacy, new code should use `Reactor` directly"; the Reactor is
//!   now gone and this is the one that has production callers.
//! - [`ring`] — the bounded event ring.
//!
//! # What used to be here
//!
//! A `Reactor` with a `Handler` trait, a `DependencyGraph` for topological
//! ordering, and four built-in handlers — 3,076 lines. It was wired into the
//! turn loop at four points and dispatched on every tool call and LLM call.
//!
//! It never ran anything. Outside its own tests nothing implemented `Handler`
//! and nothing called `register`, so every `emit` returned
//! `Completed { handler_count: 0 }` and its cancel and fail-closed arms were
//! unreachable. Session-scoped extension is `crucible.on` in the Lua registry;
//! that is the path with production handlers, and the tests that looked like
//! Reactor coverage were Lua tests standing beside it.
//!
//! Removed 2026-08-20. A `subscriber` module went with it a day later: it
//! carried `HandlerResult`, `EventFilter`, `SubscriptionId` and six more
//! types, none of which any caller named. Its header described an
//! `EventSubscriber` trait that the file did not define.

pub mod emitter;
pub mod markdown;
pub mod ring;
pub mod session_event;

// Re-exports for convenient access

// New unified Handler system

// Dependency graph for handler ordering

// Reactor (central event loop)

// Built-in handlers

// Legacy emitter exports
pub use emitter::{
    EmitOutcome, EmitResult, EventEmitter, EventError, HandlerErrorInfo, NoOpEmitter,
    SharedEventBus,
};

// Session event types
pub use session_event::{
    EntityType, EventCategory, FileChangeKind, InputType, InternalSessionEvent, NoteChangeType,
    NotePayload, Priority, ScriptingEvent, SessionEvent, SessionEventConfig, TerminalStream,
    ToolCall, ToolProvider,
};

// Ring buffer for event storage
pub use ring::{EventRing, OverflowCallback};

// Event markdown serialization
pub use markdown::{MarkdownParseError, MarkdownParseResult};
