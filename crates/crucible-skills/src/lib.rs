//! # Crucible Skills
//!
//! Agent Skills discovery, parsing, and indexing for Crucible.
//!
//! Supports the [Agent Skills](https://agentskills.io) format.

pub mod discovery;
mod error;
pub mod parser;
pub mod types;

pub use discovery::{FolderDiscovery, SearchPath};
pub use error::{SkillError, SkillResult};
pub use parser::SkillParser;
pub use types::{ResolvedSkill, Skill, SkillScope, SkillSource};

#[cfg(feature = "test-utils")]
pub mod test_utils;
