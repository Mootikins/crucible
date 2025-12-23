//! # Crucible Skills
//!
//! Agent Skills discovery, parsing, and indexing for Crucible.
//!
//! Supports the [Agent Skills](https://agentskills.io) format.

pub mod discovery;
mod error;
pub mod parser;
pub mod types;

#[cfg(feature = "storage")]
pub mod storage;

#[cfg(feature = "embeddings")]
pub mod embedding;

pub use discovery::{FolderDiscovery, SearchPath};
pub use error::{SkillError, SkillResult};
pub use parser::SkillParser;
pub use types::{ResolvedSkill, Skill, SkillScope, SkillSource};

#[cfg(feature = "storage")]
pub use storage::SkillStore;

#[cfg(feature = "embeddings")]
pub use embedding::{
    embed_skill, search_skills_semantic, SkillEmbeddingStore, SkillSearchResult, SkillSearchStore,
};

#[cfg(feature = "test-utils")]
pub mod test_utils;
