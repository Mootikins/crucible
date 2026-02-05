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

/// A registered project — a directory the user works on.
///
/// Projects group sessions by workspace path and provide metadata
/// for display in the UI (name, attached kilns, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Canonical path to the project root directory
    pub path: PathBuf,
    /// Human-readable name (from WorkspaceConfig.workspace.name or dirname)
    pub name: String,
    /// Attached kilns (from WorkspaceConfig or auto-discovered .crucible/)
    #[serde(default)]
    pub kilns: Vec<PathBuf>,
    /// When this project was last accessed
    pub last_accessed: DateTime<Utc>,
    /// SCM/repository information (if detected)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<RepositoryInfo>,
}

/// Information about the git repository containing this project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryInfo {
    /// Path to the repository root (where .git is, or main repo for worktrees)
    pub root: PathBuf,
    /// Primary remote URL (usually "origin"), if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Whether this project is in a git worktree (not the main checkout)
    #[serde(default)]
    pub is_worktree: bool,
    /// For worktrees: path to the main repository's .git directory
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
        self.kilns.push(kiln);
        self
    }

    pub fn with_kilns(mut self, kilns: Vec<PathBuf>) -> Self {
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

    pub fn repository_id(&self) -> Option<&PathBuf> {
        self.repository
            .as_ref()
            .map(|r| r.main_repo_git_dir.as_ref().unwrap_or(&r.root))
    }
}
