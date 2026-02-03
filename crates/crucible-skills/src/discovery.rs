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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_skill(dir: &Path, skill_name: &str, description: &str) {
        let skill_dir = dir.join(skill_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            "---\nname: {skill_name}\ndescription: {description}\n---\n\nInstructions for {skill_name}.\n"
        );
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn discover_single_skill() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();
        write_skill(&skills_dir, "commit", "Create commits");

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        assert_eq!(resolved.len(), 1);
        let skill = &resolved["commit"];
        assert_eq!(skill.skill.name, "commit");
        assert_eq!(skill.skill.description, "Create commits");
        assert!(skill.shadowed.is_empty());
    }

    #[test]
    fn discover_multiple_skills() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();
        write_skill(&skills_dir, "commit", "Create commits");
        write_skill(&skills_dir, "review", "Review code");
        write_skill(&skills_dir, "deploy", "Deploy to prod");

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        assert_eq!(resolved.len(), 3);
        assert!(resolved.contains_key("commit"));
        assert!(resolved.contains_key("review"));
        assert!(resolved.contains_key("deploy"));
    }

    #[test]
    fn higher_scope_shadows_lower() {
        let tmp = TempDir::new().unwrap();

        let personal_dir = tmp.path().join("personal");
        std::fs::create_dir(&personal_dir).unwrap();
        write_skill(&personal_dir, "commit", "Personal commit style");

        let workspace_dir = tmp.path().join("workspace");
        std::fs::create_dir(&workspace_dir).unwrap();
        write_skill(&workspace_dir, "commit", "Workspace commit style");

        let discovery = FolderDiscovery::new(vec![
            SearchPath::new(personal_dir, SkillScope::Personal),
            SearchPath::new(workspace_dir, SkillScope::Workspace),
        ]);
        let resolved = discovery.discover().unwrap();

        assert_eq!(resolved.len(), 1);
        let commit = &resolved["commit"];
        assert_eq!(commit.skill.description, "Workspace commit style");
        assert_eq!(commit.shadowed.len(), 1);
    }

    #[test]
    fn kiln_scope_shadows_workspace_and_personal() {
        let tmp = TempDir::new().unwrap();

        let personal = tmp.path().join("personal");
        std::fs::create_dir(&personal).unwrap();
        write_skill(&personal, "review", "Personal review");

        let workspace = tmp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        write_skill(&workspace, "review", "Workspace review");

        let kiln = tmp.path().join("kiln");
        std::fs::create_dir(&kiln).unwrap();
        write_skill(&kiln, "review", "Kiln review");

        let discovery = FolderDiscovery::new(vec![
            SearchPath::new(personal, SkillScope::Personal),
            SearchPath::new(workspace, SkillScope::Workspace),
            SearchPath::new(kiln, SkillScope::Kiln),
        ]);
        let resolved = discovery.discover().unwrap();

        let review = &resolved["review"];
        assert_eq!(review.skill.description, "Kiln review");
        assert_eq!(review.shadowed.len(), 2);
    }

    #[test]
    fn nonexistent_path_skipped() {
        let discovery = FolderDiscovery::new(vec![SearchPath::new(
            PathBuf::from("/nonexistent/path/skills"),
            SkillScope::Personal,
        )]);
        let resolved = discovery.discover().unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn empty_directory_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn malformed_skill_skipped_gracefully() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();

        // Write a valid skill
        write_skill(&skills_dir, "good-skill", "A valid skill");

        // Write a malformed SKILL.md (no frontmatter)
        let bad_dir = skills_dir.join("bad-skill");
        std::fs::create_dir(&bad_dir).unwrap();
        std::fs::write(bad_dir.join("SKILL.md"), "No frontmatter here").unwrap();

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        // Only the valid skill should be present
        assert_eq!(resolved.len(), 1);
        assert!(resolved.contains_key("good-skill"));
    }

    #[test]
    fn search_path_with_agent() {
        let sp =
            SearchPath::new(PathBuf::from("/test"), SkillScope::Workspace).with_agent("claude");

        assert_eq!(sp.agent.as_deref(), Some("claude"));
        assert_eq!(sp.scope, SkillScope::Workspace);
    }

    #[test]
    fn content_hash_populated() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();
        write_skill(&skills_dir, "hashed", "Test hashing");

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        let skill = &resolved["hashed"];
        assert!(!skill.skill.source.content_hash.is_empty());
        // SHA-256 hex digest is 64 chars
        assert_eq!(skill.skill.source.content_hash.len(), 64);
    }

    #[test]
    fn with_default_paths_includes_personal() {
        let tmp = TempDir::new().unwrap();
        let discovery = FolderDiscovery::with_default_paths(tmp.path(), None);

        // Should not panic, and discover should work on nonexistent paths
        let resolved = discovery.discover().unwrap();
        assert!(resolved.is_empty());
    }
}
