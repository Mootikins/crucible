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
//! - [`entry`] — WHICH roots there are, and the priority of each.
//! - [`resolve`] — the join. [`search_sources`] sorts by priority, for every
//!   kind; [`crate::sources`] names the entries.
//!
//! Lives in `crucible-core` because the CLI and the daemon both resolve paths;
//! `paths.rs` and `runtime_roots.rs` are here for the same reason.

pub mod asset;
pub mod build;
pub mod entry;
pub mod resolve;

pub use asset::{EntryShape, RuntimeAsset};
pub use build::{build_path, KilnRoot, PathInputs};
pub use entry::{
    default_priority, level_priority, EntryKind, LevelPriorities, Origin, Priority, PriorityLevel,
    RuntimeEntry, SearchPath,
};
pub use resolve::{name_clashes, search_paths, search_sources};
