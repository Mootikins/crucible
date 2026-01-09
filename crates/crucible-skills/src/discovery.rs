//! Folder-based skill discovery with priority ordering

use crate::error::{SkillError, SkillResult};
use crate::parser::SkillParser;
use crate::types::{ResolvedSkill, Skill, SkillScope, SkillSource};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

/// A search path with its scope/priority
#[derive(Debug, Clone)]
pub struct SearchPath {
    pub path: PathBuf,
    pub scope: SkillScope,
    pub agent: Option<String>,
}

impl SearchPath {
    pub fn new(path: PathBuf, scope: SkillScope) -> Self {
        Self {
            path,
            scope,
            agent: None,
        }
    }

    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }
}

/// Folder-based discovery with priority ordering
pub struct FolderDiscovery {
    search_paths: Vec<SearchPath>,
    parser: SkillParser,
}

impl FolderDiscovery {
    pub fn new(search_paths: Vec<SearchPath>) -> Self {
        let mut paths = search_paths;
        paths.sort_by_key(|p| p.scope);
        Self {
            search_paths: paths,
            parser: SkillParser::new(),
        }
    }

    /// Create a FolderDiscovery with default search paths
    ///
    /// Searches:
    /// - `~/.config/crucible/skills/` (personal)
    /// - `<workspace>/.<agent>/skills/` for each known agent (workspace)
    /// - `<kiln>/skills/` if kiln path provided (kiln)
    pub fn with_default_paths(workspace: &Path, kiln: Option<&Path>) -> Self {
        let paths = default_discovery_paths(Some(workspace), kiln);
        Self::new(paths)
    }

    pub fn discover(&self) -> SkillResult<HashMap<String, ResolvedSkill>> {
        let mut resolved: HashMap<String, ResolvedSkill> = HashMap::new();

        for search_path in &self.search_paths {
            if !search_path.path.exists() {
                debug!("Skipping non-existent path: {:?}", search_path.path);
                continue;
            }

            for skill in self.discover_in_path(search_path)? {
                let name = skill.name.clone();
                resolved
                    .entry(name)
                    .and_modify(|existing| {
                        if skill.source.scope >= existing.skill.source.scope {
                            existing.shadowed.push(existing.skill.source.path.clone());
                            existing.skill = skill.clone();
                        }
                    })
                    .or_insert_with(|| ResolvedSkill {
                        skill,
                        shadowed: vec![],
                    });
            }
        }
        Ok(resolved)
    }

    fn discover_in_path(&self, search_path: &SearchPath) -> SkillResult<Vec<Skill>> {
        let mut skills = Vec::new();
        let pattern = search_path.path.join("*/SKILL.md");
        let pattern_str = pattern.to_string_lossy();

        for entry in glob::glob(&pattern_str)
            .map_err(|e| SkillError::DiscoveryError(format!("Invalid glob pattern: {}", e)))?
        {
            let skill_md_path =
                entry.map_err(|e| SkillError::DiscoveryError(format!("Glob error: {}", e)))?;

            match self.parse_skill_file(&skill_md_path, search_path) {
                Ok(skill) => skills.push(skill),
                Err(e) => debug!("Failed to parse {:?}: {}", skill_md_path, e),
            }
        }
        Ok(skills)
    }

    fn parse_skill_file(&self, path: &Path, search_path: &SearchPath) -> SkillResult<Skill> {
        let content = std::fs::read_to_string(path).map_err(|e| SkillError::ReadError {
            path: path.to_path_buf(),
            source: e,
        })?;

        let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));

        let source = SkillSource {
            agent: search_path.agent.clone(),
            scope: search_path.scope,
            path: path.to_path_buf(),
            content_hash,
        };

        self.parser.parse(&content, source)
    }
}

/// Build default discovery paths for Crucible
pub fn default_discovery_paths(workspace: Option<&Path>, kiln: Option<&Path>) -> Vec<SearchPath> {
    let mut paths = Vec::new();

    if let Some(config_dir) = dirs::config_dir() {
        paths.push(
            SearchPath::new(
                config_dir.join("crucible").join("skills"),
                SkillScope::Personal,
            )
            .with_agent("crucible"),
        );
    }

    if let Some(ws) = workspace {
        for agent in &["claude", "codex", "opencode", "crucible"] {
            let agent_path = ws.join(format!(".{}", agent)).join("skills");
            if agent_path.exists() {
                paths.push(SearchPath::new(agent_path, SkillScope::Workspace).with_agent(*agent));
            }
        }
    }

    if let Some(k) = kiln {
        paths.push(SearchPath::new(k.join("skills"), SkillScope::Kiln).with_agent("crucible"));
    }

    paths
}
