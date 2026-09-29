//! # Crucible Skills
//!
//! Agent Skills discovery, parsing, and indexing for Crucible.
//!
//! Supports the [Agent Skills](https://agentskills.io) format.

pub mod context;
pub mod discovery;
pub mod parser;
pub mod types;

pub use context::{format_skills_for_context, skill_instructions};
pub use discovery::{FolderDiscovery, SearchPath};
pub use parser::SkillParser;
pub use types::{ResolvedSkill, Skill, SkillScope, SkillSource};
