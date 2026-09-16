//! Project types for workspace/directory registration
//!
//! Projects are lightweight wrappers over workspace paths that provide
//! metadata and session grouping for the web UI and CLI.
//!
//! ## Taxonomy
//!
//! - **Project**: A directory the user works on (registered in daemon)
//! - **Workspace**: The working directory for a session (may equal project path)
//! - **Repository**: A git repo that may contain one or more projects/worktrees

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectKiln {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub path: PathBuf,
    /// The kiln's registry name. A kiln that declares none sends NO `name`
    /// key, so a reader must treat the field as absent rather than null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A registered project — a directory the user works on.
///
/// Projects group sessions by workspace path and provide metadata
/// for display in the UI (name, attached kilns, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Project {
    /// Canonical path to the project root directory
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub path: PathBuf,
    /// Human-readable name (from `ProjectConfig.project.name` or dirname)
    pub name: String,
    /// Attached kilns (from `ProjectConfig` or auto-discovered .crucible/)
    ///
    /// `default` covers a stored project written before the field existed. It
    /// is always WRITTEN, empty or not, so a reader never sees it absent.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub kilns: Vec<ProjectKiln>,
    /// When this project was last accessed
    pub last_accessed: DateTime<Utc>,
    /// SCM/repository information (if detected)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryInfo>,
}

/// Information about the git repository containing this project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RepositoryInfo {
    /// Path to the repository root (where .git is, or main repo for worktrees)
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PathBuf,
    /// Primary remote URL (usually "origin"), if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Whether this project is in a git worktree (not the main checkout).
    /// Always written, so `required` rather than optional.
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub is_worktree: bool,
    /// For worktrees: path to the main repository's .git directory
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub main_repo_git_dir: Option<PathBuf>,
}

impl Project {
    pub fn new(path: PathBuf, name: String) -> Self {
        Self {
            path,
            name,
            kilns: Vec::new(),
            last_accessed: Utc::now(),
            repository: None,
        }
    }

    pub fn with_kiln(mut self, kiln: PathBuf) -> Self {
        self.kilns.push(ProjectKiln {
            path: kiln,
            name: None,
        });
        self
    }

    pub fn with_kilns(mut self, kilns: Vec<ProjectKiln>) -> Self {
        self.kilns = kilns;
        self
    }

    pub fn with_repository(mut self, repo: RepositoryInfo) -> Self {
        self.repository = Some(repo);
        self
    }

    pub fn touch(&mut self) {
        self.last_accessed = Utc::now();
    }
}
