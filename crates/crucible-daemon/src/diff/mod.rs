//! The daemon side of the diffset: the code that computes the files of a
//! [`crucible_core::diff::Diffset`] from its source.
//!
//! Each source has its own module. `branch` reads git.

pub mod branch;
