//! Error types for crucible-skills

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("Failed to read skill file: {path}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse SKILL.md frontmatter: {path}")]
    ParseError {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("Invalid skill: {reason}")]
    ValidationError { reason: String },

    #[error("Discovery error: {0}")]
    DiscoveryError(String),

    #[error("Skill not found: {name}")]
    NotFound { name: String },
}

pub type SkillResult<T> = Result<T, SkillError>;
