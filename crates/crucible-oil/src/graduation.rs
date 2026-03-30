//! Graduation system — REMOVED
//!
//! Graduation (content moving from viewport to stdout scrollback) is now handled
//! at the app layer via drain-based container graduation. The ContainerList drains
//! completed containers from the front, renders them, and writes to stdout.
//!
//! The old system (Node::Static, GraduationState, key-based tracking) has been
//! replaced by this simpler model that follows Ink's architecture.
