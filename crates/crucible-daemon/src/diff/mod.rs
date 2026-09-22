//! The daemon side of the diffset: the code that computes the files of a
//! [`crucible_core::diff::Diffset`] from its source.
//!
//! Each source has its own module. `branch` reads git. `comments` stores
//! the comments of each diffset. `context` builds the block of a comment that
//! a chat message attaches.

pub mod branch;
pub mod comments;
pub mod context;
