//! One ordered list of runtime roots, and one table of what lives under them.
//!
//! Five resolvers used to answer "where does this asset come from", each with
//! its own root list and its own precedence rule. They drifted, and the drift
//! shipped bugs: `cru agents list` advertised cards the daemon would not
//! resolve, an installed `cru` found none of its bundled plugins, and a theme
//! copied by `cru setup` was never listed.
//!
//! - [`asset`] — WHAT lives under a root, as one enumerated table with a gate.
//! - [`build`] — turning config, environment and session into that list.
//! - [`entry`] — WHICH roots there are, in precedence order.
//! - [`resolve`] — the join. Position in the list is precedence, for every
//!   kind.
//!
//! Lives in `crucible-core` because the CLI and the daemon both resolve paths;
//! `paths.rs` and `runtime_roots.rs` are here for the same reason.

pub mod asset;
pub mod build;
pub mod entry;
pub mod resolve;

pub use asset::{EntryShape, RuntimeAsset};
pub use build::{build_path, PathInputs};
pub use entry::{EntryKind, Origin, RuntimeEntry, SearchPath};
pub use resolve::search_paths;
