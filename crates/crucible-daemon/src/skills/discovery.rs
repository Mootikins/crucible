//! Folder-based skill discovery with priority ordering

use crate::skills::error::{SkillError, SkillResult};
use crate::skills::parser::SkillParser;
use crate::skills::types::{ResolvedSkill, Skill, SkillScope, SkillSource};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Maximum size of a `SKILL.md` we'll read into memory.
///
/// Skills become LLM instructions, so a pathological file at the discovery
/// path (intentionally hostile or accidentally generated) would either OOM
/// the daemon at startup or balloon every prompt. 256 KB is well above any
/// reasonable human-authored skill and still bounded.
const SKILL_MAX_BYTES: u64 = 256 * 1024;

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
        let paths = default_discovery_paths(Some(workspace), kiln, dirs::home_dir().as_deref());
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
        // Reject symlinks. A SKILL.md (or its parent directory) symlinked
        // to anything sensitive — `~/.ssh/id_rsa`, another user's home,
        // arbitrary system files — would otherwise be read into LLM context
        // as instructions. Cross-harness discovery makes this realistic:
        // any unrelated tool that writes to `~/.claude/skills/...` becomes
        // a vector.
        let file_meta = std::fs::symlink_metadata(path).map_err(|e| SkillError::ReadError {
            path: path.to_path_buf(),
            source: e,
        })?;
        if file_meta.file_type().is_symlink() {
            warn!(
                path = %path.display(),
                "Skipping symlinked SKILL.md (security policy)"
            );
            return Err(SkillError::DiscoveryError(format!(
                "skipped symlinked SKILL.md: {}",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            if let Ok(parent_meta) = std::fs::symlink_metadata(parent) {
                if parent_meta.file_type().is_symlink() {
                    warn!(
                        path = %parent.display(),
                        "Skipping skill in symlinked directory (security policy)"
                    );
                    return Err(SkillError::DiscoveryError(format!(
                        "skipped skill in symlinked directory: {}",
                        parent.display()
                    )));
                }
            }
        }

        // Cap file size before reading. Prevents a 2 GB SKILL.md from
        // OOMing the daemon at discovery time.
        if file_meta.len() > SKILL_MAX_BYTES {
            warn!(
                path = %path.display(),
                size = file_meta.len(),
                limit = SKILL_MAX_BYTES,
                "Skipping oversized SKILL.md"
            );
            return Err(SkillError::DiscoveryError(format!(
                "SKILL.md exceeds {} bytes: {}",
                SKILL_MAX_BYTES,
                path.display()
            )));
        }

        let content = std::fs::read_to_string(path).map_err(|e| SkillError::ReadError {
            path: path.to_path_buf(),
            source: e,
        })?;

        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));

        let source = SkillSource {
            agent: search_path.agent.clone(),
            scope: search_path.scope,
            path: path.to_path_buf(),
            content_hash,
        };

        self.parser.parse(&content, source)
    }
}

/// Add skill paths from other coding-agent harnesses' home directories.
///
/// Pi explicitly cross-discovers other harnesses' skill libraries (see
/// `pi-mono/packages/coding-agent/docs/skills.md` § "Using Skills from
/// Other Harnesses"). We do the same: users invest in skill collections
/// inside `~/.claude/skills`, `~/.codex/skills`, etc.; refusing to read
/// them deepens ecosystem fragmentation for no real benefit.
///
/// Known harness shapes:
/// - `claude`, `codex`, `opencode` → `~/.<harness>/skills/`
/// - `pi`                          → `~/.pi/agent/skills/`
///
/// Unknown harness names are ignored (the helper only knows shapes it
/// has been taught). Missing directories are silently skipped.
///
/// All cross-harness paths get `SkillScope::Personal` — they sit at the
/// same priority as `~/.config/crucible/skills`, lower than workspace
/// and kiln. The `agent` tag records the source harness so callers can
/// disambiguate name clashes (`commit` from Claude vs Crucible) and
/// show provenance.
pub fn cross_harness_home_paths(home: &Path, harnesses: &[&str]) -> Vec<SearchPath> {
    let mut paths = Vec::new();
    for harness in harnesses {
        let candidate = match *harness {
            "claude" | "codex" | "opencode" => home.join(format!(".{harness}")).join("skills"),
            "pi" => home.join(".pi").join("agent").join("skills"),
            _ => continue,
        };
        if candidate.exists() {
            paths.push(SearchPath::new(candidate, SkillScope::Personal).with_agent(*harness));
        }
    }
    paths
}

/// Build default discovery paths for Crucible.
///
/// In production, callers pass `dirs::home_dir().as_deref()` for `home`.
/// Tests inject a tempdir so they don't depend on the host's real
/// `~/.claude/skills` / `~/.codex/skills` / `~/.pi/agent/skills` contents.
pub fn default_discovery_paths(
    workspace: Option<&Path>,
    kiln: Option<&Path>,
    home: Option<&Path>,
) -> Vec<SearchPath> {
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

    if let Some(home) = home {
        // Cross-harness discovery (reading skills from `~/.claude/skills`,
        // `~/.codex/skills`, `~/.pi/agent/skills`, `~/.opencode/skills`) is
        // **opt-in**. Skill contents become LLM instructions, so silently
        // sourcing prompts from another tool's config dir is a meaningful
        // attack surface: any installer that drops a file into those paths
        // (legitimately for the other tool) can inject into Crucible.
        //
        // Set `CRUCIBLE_CROSS_HARNESS_SKILLS=1` (or `true`/`on`) to enable.
        let enabled = matches!(
            std::env::var("CRUCIBLE_CROSS_HARNESS_SKILLS").as_deref(),
            Ok("1") | Ok("true") | Ok("on")
        );
        if enabled {
            let extras = cross_harness_home_paths(home, &["claude", "codex", "opencode", "pi"]);
            if !extras.is_empty() {
                let names: Vec<&str> = extras.iter().filter_map(|p| p.agent.as_deref()).collect();
                tracing::info!(
                    harnesses = ?names,
                    "Discovered skill libraries from other coding-agent harnesses (CRUCIBLE_CROSS_HARNESS_SKILLS opt-in is set)"
                );
            }
            paths.extend(extras);
        }
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

    let runtime_roots = match std::env::var("CRUCIBLE_RUNTIME") {
        Ok(base) => vec![PathBuf::from(base)],
        Err(_) => crucible_core::runtime_roots::for_current_exe(),
    };
    paths.extend(runtime_skill_paths(&runtime_roots));

    paths
}

/// The `<root>/<bundle>/skills` directories under any of `roots`.
///
/// Separate from [`default_discovery_paths`] so the layout question — which
/// roots exist, in what order — is testable without `current_exe()`. That is
/// the half that was wrong: only the dev tree was ever scanned, so an
/// installed `cru` found no bundled skills and said nothing about it.
fn runtime_skill_paths(roots: &[PathBuf]) -> Vec<SearchPath> {
    let mut paths = Vec::new();
    for root in roots {
        for entry in std::fs::read_dir(root).ok().into_iter().flatten().flatten() {
            let skills_path = entry.path().join("skills");
            if entry.path().is_dir() && skills_path.exists() {
                debug!("Adding runtime skills path: {:?}", skills_path);
                paths
                    .push(SearchPath::new(skills_path, SkillScope::Builtin).with_agent("crucible"));
            }
        }
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

    /// Candidate roots carry `..` components by construction, so compare
    /// what they resolve to rather than how they are spelled.
    fn same_dir(a: &Path, b: &Path) -> bool {
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    /// An installed `cru` must find the skills shipped alongside it.
    ///
    /// The resolver tried `$CRUCIBLE_RUNTIME` else `<exe>/../../runtime` — the
    /// dev layout only. For `~/.local/bin/cru` that is `~/runtime`, so the
    /// bundled `crucible-help` skills never loaded for anyone who installed
    /// Crucible rather than building it, with no error to show for it. The
    /// two sibling resolvers (`runtime_defaults`, `daemon_plugins`) both
    /// already tried the installed layout first.
    #[test]
    fn an_installed_binary_finds_the_bundled_runtime_skills() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();
        let bin = prefix.join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let bundle = prefix.join("share/crucible/runtime/crucible-help/skills");
        std::fs::create_dir_all(&bundle).unwrap();
        write_skill(&bundle, "cru-help", "Explains Crucible commands");

        let roots = crucible_core::runtime_roots::exe_relative(&bin);
        let found = runtime_skill_paths(&roots);

        assert!(
            found.iter().any(|p| same_dir(&p.path, &bundle)),
            "installed-layout skills must be discovered, got: {:?}",
            found.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
    }

    /// The dev tree still resolves — the installed layout is an addition, not
    /// a replacement, and this is the layout every contributor runs.
    #[test]
    fn a_dev_tree_binary_still_finds_the_bundled_runtime_skills() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path();
        let exe_dir = repo.join("target/debug");
        std::fs::create_dir_all(&exe_dir).unwrap();

        let bundle = repo.join("runtime/crucible-help/skills");
        std::fs::create_dir_all(&bundle).unwrap();
        write_skill(&bundle, "cru-help", "Explains Crucible commands");

        let found = runtime_skill_paths(&crucible_core::runtime_roots::exe_relative(&exe_dir));

        assert!(
            found.iter().any(|p| same_dir(&p.path, &bundle)),
            "dev-layout skills must still be discovered, got: {:?}",
            found.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cross_harness_home_paths_includes_known_harness_skill_dirs() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();

        let claude_skills = home.join(".claude").join("skills");
        std::fs::create_dir_all(&claude_skills).unwrap();
        write_skill(&claude_skills, "claude-commit", "Claude commit skill");

        let codex_skills = home.join(".codex").join("skills");
        std::fs::create_dir_all(&codex_skills).unwrap();
        write_skill(&codex_skills, "codex-review", "Codex review skill");

        let opencode_skills = home.join(".opencode").join("skills");
        std::fs::create_dir_all(&opencode_skills).unwrap();
        write_skill(&opencode_skills, "opencode-debug", "OpenCode debug skill");

        // Pi's home-skill path is one level deeper than the others.
        let pi_skills = home.join(".pi").join("agent").join("skills");
        std::fs::create_dir_all(&pi_skills).unwrap();
        write_skill(&pi_skills, "pi-plan", "Pi plan skill");

        let paths = cross_harness_home_paths(home, &["claude", "codex", "opencode", "pi"]);
        let by_agent: HashMap<String, &SearchPath> = paths
            .iter()
            .map(|p| (p.agent.clone().unwrap_or_default(), p))
            .collect();

        assert_eq!(
            paths.len(),
            4,
            "expected one path per harness, got {paths:?}"
        );
        assert_eq!(by_agent["claude"].path, claude_skills);
        assert_eq!(by_agent["codex"].path, codex_skills);
        assert_eq!(by_agent["opencode"].path, opencode_skills);
        assert_eq!(by_agent["pi"].path, pi_skills);

        // All cross-harness home paths are Personal scope (user-level
        // libraries, lower priority than workspace/kiln).
        for p in &paths {
            assert_eq!(p.scope, SkillScope::Personal);
        }
    }

    #[test]
    fn cross_harness_home_paths_skips_missing_dirs() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();

        let claude_skills = home.join(".claude").join("skills");
        std::fs::create_dir_all(&claude_skills).unwrap();
        // Intentionally do not create codex or pi dirs.

        let paths = cross_harness_home_paths(home, &["claude", "codex", "pi"]);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].agent.as_deref(), Some("claude"));
    }

    #[test]
    fn cross_harness_home_paths_unknown_harness_is_skipped() {
        let tmp = TempDir::new().unwrap();
        // Even if a "made-up" harness dir exists, the helper only
        // knows the shapes of harnesses it lists explicitly.
        let weird = tmp.path().join(".made-up").join("skills");
        std::fs::create_dir_all(&weird).unwrap();

        let paths = cross_harness_home_paths(tmp.path(), &["made-up"]);
        assert!(
            paths.is_empty(),
            "unknown harnesses should not contribute paths"
        );
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

    /// A skill you wrote beats one Crucible shipped.
    ///
    /// Bundled runtime skills were tagged `SkillScope::Kiln` — the same scope
    /// as `<kiln>/skills` — so name collisions were decided by search-path
    /// order rather than by precedence, and the bundled one won: `discover`
    /// replaces on `>=`, and runtime paths are pushed after the kiln's. Your
    /// own `review` skill lost to a shipped `review` and nothing said so.
    #[test]
    fn a_bundled_skill_never_shadows_one_you_wrote() {
        let tmp = TempDir::new().unwrap();

        let bundled = tmp.path().join("runtime");
        std::fs::create_dir(&bundled).unwrap();
        write_skill(&bundled, "review", "Bundled review");

        let kiln = tmp.path().join("kiln");
        std::fs::create_dir(&kiln).unwrap();
        write_skill(&kiln, "review", "Kiln review");

        let discovery = FolderDiscovery::new(vec![
            SearchPath::new(kiln, SkillScope::Kiln),
            SearchPath::new(bundled, SkillScope::Builtin),
        ]);
        let resolved = discovery.discover().unwrap();

        assert_eq!(
            resolved["review"].skill.description, "Kiln review",
            "the kiln's own skill must win"
        );
    }

    /// …and a bundled skill with no competition still loads.
    #[test]
    fn a_bundled_skill_loads_when_nothing_shadows_it() {
        let tmp = TempDir::new().unwrap();
        let bundled = tmp.path().join("runtime");
        std::fs::create_dir(&bundled).unwrap();
        write_skill(&bundled, "cru-help", "Explains Crucible");

        let discovery = FolderDiscovery::new(vec![SearchPath::new(bundled, SkillScope::Builtin)]);
        let resolved = discovery.discover().unwrap();

        assert_eq!(resolved["cru-help"].skill.description, "Explains Crucible");
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
        // Inject an empty home dir so the test isn't affected by the
        // host's real ~/.claude / ~/.codex / ~/.pi skill libraries.
        let paths = default_discovery_paths(Some(tmp.path()), None, Some(tmp.path()));
        let discovery = FolderDiscovery::new(paths);

        // Should not panic, and discover should work on nonexistent paths
        let resolved = discovery.discover().unwrap();
        assert!(resolved.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────
    // H2 — discovery hardening: symlinks, oversized files, opt-in default
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn discovery_skips_symlinked_skill_file() {
        // A symlink at the SKILL.md path could resolve to anything sensitive
        // (~/.ssh/id_rsa, /etc/shadow, ...). Must be skipped.
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();

        // A real, valid skill (control).
        write_skill(&skills_dir, "valid", "Valid skill");

        // Create a target file outside the skills dir, then symlink the
        // SKILL.md path to it.
        let secret_path = tmp.path().join("secret.txt");
        std::fs::write(
            &secret_path,
            "---\nname: malicious\ndescription: leaked\n---\nSECRET-CONTENT\n",
        )
        .unwrap();
        let evil_dir = skills_dir.join("evil");
        std::fs::create_dir(&evil_dir).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret_path, evil_dir.join("SKILL.md")).unwrap();

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        // The symlinked skill must NOT be discovered (control still is).
        assert!(resolved.contains_key("valid"));
        assert!(
            !resolved.contains_key("malicious"),
            "symlinked SKILL.md must be skipped"
        );
    }

    #[test]
    #[cfg(unix)]
    fn discovery_skips_skill_in_symlinked_directory() {
        // The SKILL.md is real, but its parent directory is a symlink.
        // Still treat the whole thing as suspicious and skip.
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();

        // A "real" skill directory outside the skills root.
        let real_dir = tmp.path().join("real-skill-dir");
        std::fs::create_dir(&real_dir).unwrap();
        std::fs::write(
            real_dir.join("SKILL.md"),
            "---\nname: shadowed\ndescription: via symlink\n---\nbody\n",
        )
        .unwrap();

        // Symlink the directory into the skills root.
        std::os::unix::fs::symlink(&real_dir, skills_dir.join("shadowed")).unwrap();

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        assert!(
            !resolved.contains_key("shadowed"),
            "skill in symlinked directory must be skipped"
        );
    }

    #[test]
    fn discovery_skips_oversized_skill() {
        // A 300 KB SKILL.md exceeds the 256 KB cap and must be skipped
        // without being read into memory.
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        std::fs::create_dir(&skills_dir).unwrap();

        let big_dir = skills_dir.join("big");
        std::fs::create_dir(&big_dir).unwrap();
        let mut content = String::from("---\nname: big\ndescription: too large\n---\n");
        content.push_str(&"x".repeat(300 * 1024));
        std::fs::write(big_dir.join("SKILL.md"), content).unwrap();

        // Sanity: a normal-sized skill in the same root still loads.
        write_skill(&skills_dir, "small", "Within cap");

        let discovery =
            FolderDiscovery::new(vec![SearchPath::new(skills_dir, SkillScope::Personal)]);
        let resolved = discovery.discover().unwrap();

        assert!(resolved.contains_key("small"));
        assert!(
            !resolved.contains_key("big"),
            "SKILL.md over the 256 KB cap must be skipped"
        );
    }

    #[test]
    fn cross_harness_disabled_by_default() {
        // Post-H2: cross-harness discovery is opt-in. Even with populated
        // `~/.claude/skills`, no cross-harness paths appear unless the
        // env var is explicitly enabled.
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let claude_skills = home.join(".claude").join("skills");
        std::fs::create_dir_all(&claude_skills).unwrap();
        write_skill(&claude_skills, "claude-skill", "From claude");

        // Ensure env is unset (or set to a disabling value) for this test.
        let _guard =
            crucible_core::test_support::EnvVarGuard::remove("CRUCIBLE_CROSS_HARNESS_SKILLS");

        let paths = default_discovery_paths(None, None, Some(home));
        // None of the discovered paths should reference `.claude/skills`.
        for p in &paths {
            assert!(
                !p.path.to_string_lossy().contains(".claude"),
                "cross-harness path must not appear by default: {:?}",
                p.path
            );
        }
    }

    #[test]
    fn cross_harness_enabled_by_env_var() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let claude_skills = home.join(".claude").join("skills");
        std::fs::create_dir_all(&claude_skills).unwrap();
        write_skill(&claude_skills, "claude-skill", "From claude");

        let _guard = crucible_core::test_support::EnvVarGuard::set(
            "CRUCIBLE_CROSS_HARNESS_SKILLS",
            "1".to_string(),
        );

        let paths = default_discovery_paths(None, None, Some(home));
        let has_claude = paths
            .iter()
            .any(|p| p.path.to_string_lossy().contains(".claude"));
        assert!(
            has_claude,
            "cross-harness path must appear when explicitly enabled"
        );
    }
}
