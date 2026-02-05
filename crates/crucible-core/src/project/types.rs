//! Project types for workspace/directory registration
//!
//! Projects are lightweight wrappers over workspace paths that provide
//! metadata and session grouping for the web UI and CLI.

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
}

impl Project {
    /// Create a new project with the given path and name
    pub fn new(path: PathBuf, name: String) -> Self {
        Self {
            path,
            name,
            kilns: Vec::new(),
            last_accessed: Utc::now(),
        }
    }

    /// Add a kiln path to this project
    pub fn with_kiln(mut self, kiln: PathBuf) -> Self {
        self.kilns.push(kiln);
        self
    }

    /// Set multiple kilns
    pub fn with_kilns(mut self, kilns: Vec<PathBuf>) -> Self {
        self.kilns = kilns;
        self
    }

    /// Update the last_accessed timestamp to now
    pub fn touch(&mut self) {
        self.last_accessed = Utc::now();
    }
}
