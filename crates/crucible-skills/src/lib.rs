//! # Crucible Skills
//!
//! Agent Skills discovery, parsing, and indexing for Crucible.
//!
//! Supports the [Agent Skills](https://agentskills.io) format.

pub mod types;
pub mod discovery;
pub mod parser;
mod error;

pub use error::{SkillError, SkillResult};
pub use types::{Skill, SkillScope, SkillSource, ResolvedSkill};
pub use discovery::FolderDiscovery;
pub use parser::SkillParser;

#[cfg(feature = "test-utils")]
pub mod test_utils;
