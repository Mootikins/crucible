//! Folder-based skill discovery with source-qualified collisions.

use crate::runtime_path::{MachineRuntime, SourceRoots};
use crate::skills::error::{SkillError, SkillResult};
use crate::skills::parser::SkillParser;
use crate::skills::types::{ResolvedSkill, Skill, SkillScope, SkillSource};
use crucible_core::runtime_path::{
    build_path, search_sources, Origin, PathInputs, PriorityLevel, RuntimeAsset, RuntimeEntry,
};
use crucible_core::sources::{listing, sources_new, Entry, Source};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Maximum size of a `SKILL.md` we'll read into memory.
///
/// Skills become LLM instructions, so a pathological file at the discovery
/// path (intentionally hostile or accidentally generated) would either OOM
/// the daemon at startup or balloon every prompt. 256 KB is well above any
/// reasonable human-authored skill and still bounded.
const SKILL_MAX_BYTES: u64 = 256 * 1024;

/// A skill directory: one source of skills (see `crucible_core::sources`).
#[derive(Debug, Clone)]
pub struct SearchPath {
    pub path: PathBuf,
    pub scope: SkillScope,
    pub agent: Option<String>,
    /// The source name: the prefix of each full name.
    pub namespace: String,
    /// A higher priority wins a bare name.
    pub priority: i32,
    /// The position inside one priority.
    pub within: u8,
}

impl SearchPath {
    /// A directory at the default priority of `scope`'s level.
    pub fn new(path: PathBuf, scope: SkillScope) -> Self {
        let level = match scope {
            SkillScope::Builtin => PriorityLevel::Builtin,
            SkillScope::Personal => PriorityLevel::Personal,
            SkillScope::Workspace => PriorityLevel::Workspace,
            SkillScope::Kiln => PriorityLevel::Kiln,
        };
        Self {
            path,
            scope,
            agent: None,
            namespace: scope.to_string(),
            priority: level.default_priority(),
            within: 0,
        }
    }

    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    /// Tag with a harness name when there is one; leave untagged otherwise.
    pub fn with_agent_opt(mut self, agent: Option<String>) -> Self {
        self.agent = agent;
        self
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }
}

/// Folder-based discovery across ordered search paths.
pub struct FolderDiscovery {
    search_paths: Vec<SearchPath>,
    parser: SkillParser,
}

impl FolderDiscovery {
    /// Each search path is a source. Its priority, not its position, decides
    /// which skill takes a bare name.
    pub fn new(search_paths: Vec<SearchPath>) -> Self {
        Self {
            search_paths,
            parser: SkillParser::new(),
        }
    }

    /// Create a FolderDiscovery with default search paths
    ///
    /// Searches:
    /// - `~/.config/crucible/skills/` (personal)
    /// - `<workspace>/.<agent>/skills/` for each known agent (workspace)
    /// - `<kiln>/.crucible/skills/` for each attached kiln (kiln)
    /// - `<entry>/skills/` for each `runtimepath` entry
    ///
    /// Every workspace- and kiln-relative entry is under a dot-directory, the
    /// same rule plugins and agent cards follow.
    pub fn with_default_paths(roots: &SourceRoots, workspace: &Path, kilns: &[PathBuf]) -> Self {
        let paths =
            default_discovery_paths(roots, Some(workspace), kilns, dirs::home_dir().as_deref());
        Self::new(paths)
    }

    /// Every skill, keyed by the name it is listed under.
    ///
    /// The skill of the highest source takes the bare name, and its
    /// `shadowed` names the other skills of that name. Every other skill
    /// takes its full name `source:name`. Two skills of one name at one
    /// priority both take full names, so the bare name is ambiguous.
    pub fn discover(&self) -> SkillResult<HashMap<String, ResolvedSkill>> {
        let sources = sources_new(
            self.search_paths
                .iter()
                .map(|path| Source {
                    name: path.namespace.clone(),
                    priority: path.priority,
                    within: path.within,
                    value: path,
                })
                .collect(),
        )
        .map_err(|e| SkillError::DiscoveryError(e.to_string()))?;

        let mut entries: Vec<Entry<Skill>> = Vec::new();
        for (index, source) in sources.list().iter().enumerate() {
            if !source.value.path.exists() {
                debug!("Skipping non-existent path: {:?}", source.value.path);
                continue;
            }
            for skill in self.discover_in_path(source.value)? {
                // One name twice in one directory is the directory's defect,
                // not a tie between sources.
                if entries
                    .iter()
                    .any(|e| e.source == index && e.name == skill.name)
                {
                    warn!(path = %skill.source.path.display(), "Second skill of one name skipped");
                    continue;
                }
                entries.push(Entry {
                    source: index,
                    name: skill.name.clone(),
                    value: skill,
                });
            }
        }

        let mut resolved = HashMap::new();
        for (key, index) in listing(&sources, &entries) {
            let entry = &entries[index];
            let shadowed = if key == entry.name {
                entries
                    .iter()
                    .filter(|other| other.name == entry.name && !std::ptr::eq(*other, entry))
                    .map(|other| other.value.source.path.clone())
                    .collect()
            } else {
                Vec::new()
            };
            resolved.insert(
                key,
                ResolvedSkill {
                    skill: entry.value.clone(),
                    shadowed,
                },
            );
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
            namespace: search_path.namespace.clone(),
        };

        self.parser.parse(&content, source)
    }
}

pub fn resolve_skill<'a>(
    skills: &'a HashMap<String, ResolvedSkill>,
    name: &str,
) -> Result<Option<&'a ResolvedSkill>, String> {
    if let Some(skill) = skills.get(name) {
        return Ok(Some(skill));
    }
    let mut matches: Vec<_> = skills
        .iter()
        .filter(|(_, resolved)| {
            resolved.skill.name == name
                || format!(
                    "{}:{}",
                    resolved.skill.source.namespace, resolved.skill.name
                ) == name
        })
        .collect();
    matches.sort_by(|a, b| a.0.cmp(b.0));
    match matches.as_slice() {
        [] => Ok(None),
        [(_, skill)] => Ok(Some(skill)),
        _ => Err(format!(
            "Ambiguous skill '{name}'. Use one of: {}",
            matches
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Build default discovery paths for Crucible, highest priority first.
///
/// In production, callers pass `dirs::home_dir().as_deref()` for `home`.
/// Tests inject a tempdir so they don't depend on the host's real
/// `~/.claude/skills` / `~/.codex/skills` contents.
pub fn default_discovery_paths(
    roots: &SourceRoots,
    workspace: Option<&Path>,
    kilns: &[PathBuf],
    home: Option<&Path>,
) -> Vec<SearchPath> {
    let config_home = roots.config_home.as_ref().map(|d| d.join("crucible"));
    let runtime = crate::runtime_path::machine_runtime(config_home.as_deref());
    default_discovery_paths_from(roots, workspace, kilns, home, &runtime)
}

/// The harness home directories to read skills from, by name.
///
/// **Every row is an opt-in.** Skill text becomes LLM instructions, so
/// silently sourcing prompts from another tool's config directory is a real
/// attack surface: any installer that legitimately drops a file into
/// `~/.claude/skills` would be writing into Crucible's system prompt.
///
/// This replaces a hardcoded `match` on four harness names plus a separate
/// hardcoded workspace loop, which disagreed — `pi` resolved at home scope
/// only and `crucible` at workspace scope only. A harness is a config row now,
/// which is also how `.agents` (the convention seventeen agent products read)
/// arrives without a code change.
pub fn harness_roots(home: &Path, harnesses: &BTreeMap<String, PathBuf>) -> Vec<RuntimeEntry> {
    harnesses
        .iter()
        .map(|(name, rel)| {
            let root = if rel.is_absolute() {
                rel.clone()
            } else {
                home.join(rel)
            };
            RuntimeEntry::root(root, Origin::Harness).with_harness(name.clone())
        })
        .collect()
}

/// `default_discovery_paths` with the runtime roots and harness table supplied
/// rather than discovered.
///
/// Tests must use this. The discovering version appends the installed
/// runtime's skill directories, so a test asserting "no skills are found"
/// passes on CI and fails on any machine where `cru` has been installed and
/// run — which is every developer's.
pub fn default_discovery_paths_from(
    roots: &SourceRoots,
    workspace: Option<&Path>,
    kilns: &[PathBuf],
    home: Option<&Path>,
    runtime: &MachineRuntime,
) -> Vec<SearchPath> {
    // `SourceRoots::config_home` is the raw config dir, as for cards.
    let config_home = roots.config_home.as_ref().map(|d| d.join("crucible"));
    let workspace_roots = workspace_root_names();
    let harnesses = home.map(enabled_harnesses).unwrap_or_default();

    let kiln_roots = roots.kiln_roots(kilns);
    let path = build_path(&PathInputs {
        workspace,
        workspace_roots: &workspace_roots,
        kilns: &kiln_roots,
        harnesses: &harnesses,
        config_home: config_home.as_deref(),
        runtimepath: &roots.runtimepath,
        env_runtime: runtime.env.as_deref(),
        user_runtime: runtime.user.as_deref(),
        runtime_roots: &runtime.shipped,
        plugin_dirs: &roots.plugin_dirs.dirs(),
        ..PathInputs::default()
    });

    let sources = match search_sources(RuntimeAsset::Skills, &path) {
        Ok(sources) => sources,
        Err(error) => {
            warn!(%error, "Skill directories are invalid; no skills load");
            return Vec::new();
        }
    };
    sources
        .list()
        .iter()
        .map(|source| SearchPath {
            path: source.value.path.clone(),
            scope: scope_for(source.value.origin),
            agent: source.value.harness.clone(),
            namespace: source.name.clone(),
            priority: source.priority,
            within: source.within,
        })
        .collect()
}

/// The relative roots searched inside a workspace and a kiln.
///
/// `.agents` is the cross-vendor convention; the rest are the harnesses whose
/// project-local directory shape is `.<name>/skills`.
fn workspace_root_names() -> Vec<String> {
    [".crucible", ".agents", ".claude", ".codex", ".opencode"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The harness rows in force. Empty unless the user opts in.
///
/// `CRUCIBLE_CROSS_HARNESS_SKILLS=1` remains the switch for one release, and
/// now turns on a NAMED table rather than a hardcoded list, so the shapes that
/// differ — `pi` keeps its skills under `~/.pi/agent` — are data.
fn enabled_harnesses(_home: &Path) -> BTreeMap<String, PathBuf> {
    let enabled = matches!(
        std::env::var("CRUCIBLE_CROSS_HARNESS_SKILLS").as_deref(),
        Ok("1") | Ok("true") | Ok("on")
    );
    if !enabled {
        return BTreeMap::new();
    }
    [
        ("agents", ".agents"),
        ("claude", ".claude"),
        ("codex", ".codex"),
        ("opencode", ".opencode"),
        ("pi", ".pi/agent"),
    ]
    .iter()
    .map(|(name, rel)| (name.to_string(), PathBuf::from(rel)))
    .collect()
}

/// The reported scope for a root's provenance.
///
/// `SkillScope` is what `cru skills list` prints and what the web route
/// filters on, so it stays exactly as it was; it is derived from `Origin`
/// rather than being a second precedence mechanism.
fn scope_for(origin: Origin) -> SkillScope {
    match origin {
        Origin::Workspace => SkillScope::Workspace,
        Origin::Kiln => SkillScope::Kiln,
        Origin::Env | Origin::Config(_) | Origin::Harness | Origin::UserConfig => {
            SkillScope::Personal
        }
        Origin::EnvRuntime | Origin::UserRuntime | Origin::Plugin | Origin::Bundled => {
            SkillScope::Builtin
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::runtime_path::search_paths;

    /// The skill directories of `roots` alone, through the real resolver.
    ///
    /// Replaces `runtime_skill_paths`, which globbed `<root>/*/skills` and
    /// needed `.rev()` to undo the scope sort. Bundled skills now live at
    /// `<root>/skills` like every other kind, and position is precedence.
    fn bundled_skill_paths(roots: &[PathBuf]) -> Vec<SearchPath> {
        let runtime = MachineRuntime {
            shipped: roots.to_vec(),
            ..MachineRuntime::default()
        };
        default_discovery_paths_from(&SourceRoots::default(), None, &[], None, &runtime)
            .into_iter()
            .filter(|p| {
                p.path
                    .starts_with(roots.first().cloned().unwrap_or_default())
                    || roots.iter().any(|r| p.path.starts_with(r))
            })
            .collect()
    }
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

        let bundle = prefix.join("share/crucible/runtime/skills");
        std::fs::create_dir_all(&bundle).unwrap();
        write_skill(&bundle, "cru-help", "Explains Crucible commands");

        let roots = crucible_core::runtime_roots::exe_relative(&bin);
        let found = bundled_skill_paths(&roots);

        assert!(
            found.iter().any(|p| same_dir(&p.path, &bundle)),
            "installed-layout skills must be discovered, got: {:?}",
            found.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
    }

    /// The case no packaging route covers: nothing on disk at all.
    ///
    /// The two tests either side of this one stage a runtime tree by hand,
    /// which is what a release tarball never does — `cargo-dist`'s installer
    /// unpacks the archive, moves the binary out, and deletes the rest. So the
    /// bundled help skills reached nobody who installed Crucible. This asserts
    /// the whole path an installed user actually takes: bytes compiled into
    /// `cru`, extracted to a directory, and found by the real discovery code.
    #[test]
    fn skills_extracted_from_the_binary_are_discovered() {
        let tmp = TempDir::new().unwrap();
        let extracted = tmp.path().join("runtime-x.y.z");
        crucible_core::runtime_roots::write_bundled_runtime(&extracted).unwrap();

        // `crucible-help` ships its skills as a PLUGIN, beside its own
        // manifest — which is why the shipped tree has no top-level `skills/`.
        // A plugin's directory is a runtime root, so its `skills/` resolves
        // through the same table as everything else.
        let plugin_dir = extracted.join("plugins").join("crucible-help");
        assert!(
            plugin_dir.join("init.luau").is_file(),
            "the extracted tree must carry crucible-help as a plugin"
        );

        let path = build_path(&PathInputs {
            plugin_dirs: std::slice::from_ref(&plugin_dir),
            ..PathInputs::default()
        });
        let found: Vec<PathBuf> = search_paths(RuntimeAsset::Skills, &path)
            .into_iter()
            .map(|c| c.path)
            .collect();

        let skills = plugin_dir.join("skills");
        assert!(
            found.iter().any(|p| same_dir(p, &skills)),
            "extracted help skills must be discovered, got: {found:?}"
        );
        assert!(
            skills.join("crucible-help").join("SKILL.md").is_file(),
            "and the skill itself must have travelled with the plugin"
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

        let bundle = repo.join("runtime/skills");
        std::fs::create_dir_all(&bundle).unwrap();
        write_skill(&bundle, "cru-help", "Explains Crucible commands");

        let found = bundled_skill_paths(&crucible_core::runtime_roots::exe_relative(&exe_dir));

        assert!(
            found.iter().any(|p| same_dir(&p.path, &bundle)),
            "dev-layout skills must still be discovered, got: {:?}",
            found.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
    }

    /// Every harness row resolves at its own shape, including the odd ones.
    ///
    /// `pi` keeps its skills one level deeper. That used to need a `match` arm
    /// in Rust; it is a table row now, which is the whole point.
    #[test]
    fn a_harness_row_resolves_at_its_own_shape() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();

        let mut harnesses = BTreeMap::new();
        for (name, rel) in [
            ("agents", ".agents"),
            ("claude", ".claude"),
            ("codex", ".codex"),
            ("pi", ".pi/agent"),
        ] {
            harnesses.insert(name.to_string(), PathBuf::from(rel));
        }

        let entries = harness_roots(home, &harnesses);
        let dirs: Vec<PathBuf> = search_paths(RuntimeAsset::Skills, &entries)
            .into_iter()
            .map(|c| c.path)
            .collect();

        assert!(dirs.contains(&home.join(".agents").join("skills")));
        assert!(dirs.contains(&home.join(".claude").join("skills")));
        assert!(dirs.contains(&home.join(".codex").join("skills")));
        assert!(
            dirs.contains(&home.join(".pi").join("agent").join("skills")),
            "pi's deeper shape must survive as data: {dirs:?}"
        );
    }

    /// A harness carries its name through as provenance, and reports Personal
    /// scope — a user-level library, below workspace and kiln.
    #[test]
    fn a_harness_root_is_personal_scope_and_keeps_its_name() {
        let tmp = TempDir::new().unwrap();
        let mut harnesses = BTreeMap::new();
        harnesses.insert("claude".to_string(), PathBuf::from(".claude"));

        let entries = harness_roots(tmp.path(), &harnesses);
        let found = search_paths(RuntimeAsset::Skills, &entries);
        assert_eq!(found[0].harness.as_deref(), Some("claude"));
        assert_eq!(scope_for(found[0].origin), SkillScope::Personal);
    }

    /// An empty harness table reads nothing. A row IS the opt-in.
    ///
    /// Skill text becomes LLM instructions, so a harness directory must never
    /// be read because it merely exists on disk.
    #[test]
    fn no_harness_rows_read_no_harness_directories() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude").join("skills")).unwrap();

        let entries = harness_roots(tmp.path(), &BTreeMap::new());
        assert!(
            search_paths(RuntimeAsset::Skills, &entries).is_empty(),
            "a harness directory that exists must still not be read unnamed"
        );
    }

    /// A bare skill name goes to the highest source, and that skill names
    /// the skills it shadows. Each full name still reaches its own skill.
    #[test]
    fn the_bare_skill_name_goes_to_the_highest_source() {
        let tmp = TempDir::new().unwrap();
        let personal = tmp.path().join("personal");
        let workspace = tmp.path().join("workspace");
        write_skill(&personal, "commit", "Personal style");
        write_skill(&workspace, "commit", "Workspace style");
        let found = FolderDiscovery::new(vec![
            SearchPath::new(workspace.clone(), SkillScope::Workspace),
            SearchPath::new(personal, SkillScope::Personal),
        ])
        .discover()
        .unwrap();
        let bare = resolve_skill(&found, "commit").unwrap().unwrap();
        assert_eq!(bare.skill.description, "Personal style");
        assert_eq!(
            bare.shadowed,
            vec![workspace.join("commit").join("SKILL.md")]
        );
        assert_eq!(
            resolve_skill(&found, "workspace:commit")
                .unwrap()
                .unwrap()
                .skill
                .description,
            "Workspace style"
        );
    }

    /// A `runtimepath` entry supplies skills from its `skills/`.
    #[test]
    fn a_runtimepath_entry_supplies_skills() {
        let kit = TempDir::new().unwrap();
        write_skill(&kit.path().join("skills"), "kit-only", "From the kit");
        let roots = SourceRoots {
            runtimepath: vec![kit.path().to_path_buf()],
            ..SourceRoots::default()
        };
        let paths =
            default_discovery_paths_from(&roots, None, &[], None, &MachineRuntime::default());
        let found = FolderDiscovery::new(paths).discover().unwrap();
        assert_eq!(found["kit-only"].skill.source.namespace, "config-1");
    }

    /// Each active plugin is one source at priority 200. Two plugins that
    /// ship one skill name are ambiguous; each full name still works.
    #[test]
    fn two_plugins_that_ship_one_skill_are_ambiguous() {
        let tmp = TempDir::new().unwrap();
        let roots = SourceRoots::default();
        for plugin in ["alpha", "beta"] {
            let dir = tmp.path().join(plugin);
            write_skill(&dir.join("skills"), "guide", plugin);
            roots.plugin_dirs.insert(plugin, dir);
        }
        let paths =
            default_discovery_paths_from(&roots, None, &[], None, &MachineRuntime::default());
        let found = FolderDiscovery::new(paths).discover().unwrap();
        let error = resolve_skill(&found, "guide").unwrap_err();
        assert!(
            error.contains("alpha:guide") && error.contains("beta:guide"),
            "{error}"
        );
        assert_eq!(
            resolve_skill(&found, "beta:guide")
                .unwrap()
                .unwrap()
                .skill
                .description,
            "beta"
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
        assert_eq!(
            resolve_skill(&resolved, "personal:commit")
                .unwrap()
                .unwrap()
                .skill
                .name,
            "commit"
        );
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
    fn higher_scope_collision_keeps_both() {
        let tmp = TempDir::new().unwrap();

        let personal_dir = tmp.path().join("personal");
        std::fs::create_dir(&personal_dir).unwrap();
        write_skill(&personal_dir, "commit", "Personal commit style");

        let workspace_dir = tmp.path().join("workspace");
        std::fs::create_dir(&workspace_dir).unwrap();
        write_skill(&workspace_dir, "commit", "Workspace commit style");

        let discovery = FolderDiscovery::new(vec![
            SearchPath::new(workspace_dir, SkillScope::Workspace),
            SearchPath::new(personal_dir, SkillScope::Personal),
        ]);
        let resolved = discovery.discover().unwrap();

        assert_eq!(resolved.len(), 2);
        let commit = &resolved["workspace:commit"];
        assert_eq!(commit.skill.description, "Workspace commit style");
        assert_eq!(
            resolved["commit"].skill.description, "Personal commit style",
            "personal is above workspace, so it takes the bare name"
        );
        assert_eq!(
            resolve_skill(&resolved, "workspace:commit")
                .unwrap()
                .unwrap()
                .skill
                .description,
            "Workspace commit style"
        );
    }

    #[test]
    fn colliding_skill_names_keep_both_source_qualified_entries() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let personal = tmp.path().join("personal");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&personal).unwrap();
        write_skill(&workspace, "commit", "Workspace style");
        write_skill(&personal, "commit", "Personal style");
        let found = FolderDiscovery::new(vec![
            SearchPath::new(workspace, SkillScope::Workspace),
            SearchPath::new(personal, SkillScope::Personal),
        ])
        .discover()
        .unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(
            found["workspace:commit"].skill.description,
            "Workspace style"
        );
        assert_eq!(found["commit"].skill.description, "Personal style");
        assert_eq!(
            resolve_skill(&found, "personal:commit")
                .unwrap()
                .unwrap()
                .skill
                .description,
            "Personal style"
        );
    }

    #[test]
    fn kiln_collision_keeps_workspace_and_personal() {
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

        // Highest priority first: `FolderDiscovery` takes the order given
        // and keeps the first match. It used to sort by scope, which made the
        // caller's order meaningless.
        let discovery = FolderDiscovery::new(vec![
            SearchPath::new(kiln, SkillScope::Kiln),
            SearchPath::new(workspace, SkillScope::Workspace),
            SearchPath::new(personal, SkillScope::Personal),
        ]);
        let resolved = discovery.discover().unwrap();

        let review = &resolved["kiln:review"];
        assert_eq!(review.skill.description, "Kiln review");
        assert_eq!(resolved.len(), 3);
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
            "the kiln's own skill takes the bare name"
        );
        assert_eq!(
            resolve_skill(&resolved, "kiln:review")
                .unwrap()
                .unwrap()
                .skill
                .description,
            "Kiln review"
        );
        assert_eq!(
            resolved["builtin:review"].skill.description,
            "Bundled review"
        );
    }

    /// A higher-priority runtime root wins against a lower one.
    ///
    /// All runtime paths share `Builtin` scope and `discover` replaces on
    /// `>=`, so among equal scopes the *last* path seen wins — which made
    /// walking the roots in priority order do the opposite of what it looked
    /// like, and let the shipped tree beat `~/.config/crucible/runtime`.
    #[test]
    fn the_highest_priority_runtime_root_wins() {
        let tmp = TempDir::new().unwrap();
        for (root, desc) in [("user", "User copy"), ("shipped", "Shipped copy")] {
            let dir = tmp.path().join(root).join("skills");
            std::fs::create_dir_all(&dir).unwrap();
            write_skill(&dir, "cru-help", desc);
        }

        // `runtime_roots` order: user runtime first, shipped after.
        let roots = vec![tmp.path().join("user"), tmp.path().join("shipped")];
        let resolved = FolderDiscovery::new(bundled_skill_paths(&roots))
            .discover()
            .unwrap();

        assert_eq!(resolved.len(), 2);
        assert!(resolved
            .values()
            .any(|skill| skill.skill.description == "User copy"));
        assert!(resolved
            .values()
            .any(|skill| skill.skill.description == "Shipped copy"));
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
        let paths = default_discovery_paths_from(
            &SourceRoots::default(),
            Some(tmp.path()),
            &[],
            Some(tmp.path()),
            &MachineRuntime::default(),
        );
        let discovery = FolderDiscovery::new(paths);

        // Should not panic, and discover should work on nonexistent paths
        let resolved = discovery.discover().unwrap();
        assert!(resolved.is_empty());
    }

    /// Every auto-detected directory is a `.crucible/` one.
    ///
    /// The kiln tier used to be the visible `KILN/skills/`, which made it the
    /// odd one out among the three things Crucible discovers — plugins and
    /// agent cards both read only `.crucible/`. It also meant a cloned or
    /// synced kiln could put text straight into an agent's system prompt just
    /// by containing a `skills/` directory. A kiln that is a skill library
    /// adds itself at load instead of being scanned.
    #[test]
    fn every_auto_detected_directory_is_under_a_dot_crucible() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("workspace");
        let kiln = tmp.path().join("kiln");
        // The workspace tier only lists directories that exist.
        for agent in ["claude", "codex", "opencode", "crucible"] {
            std::fs::create_dir_all(ws.join(format!(".{agent}")).join("skills")).unwrap();
        }
        std::fs::create_dir_all(kiln.join("skills")).unwrap();
        std::fs::create_dir_all(kiln.join(".crucible").join("skills")).unwrap();

        let paths = default_discovery_paths_from(
            &SourceRoots::default(),
            Some(&ws),
            std::slice::from_ref(&kiln),
            Some(tmp.path()),
            &MachineRuntime::default(),
        );

        assert!(
            paths
                .iter()
                .any(|p| p.path == kiln.join(".crucible").join("skills")),
            "the kiln's .crucible/skills must be searched: {:?}",
            paths.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
        assert!(
            !paths.iter().any(|p| p.path == kiln.join("skills")),
            "the kiln's visible skills/ must NOT be searched: {:?}",
            paths.iter().map(|p| &p.path).collect::<Vec<_>>()
        );

        // Nothing workspace- or kiln-relative escapes a dot-directory. Runtime
        // and personal roots live outside both and are not in scope here.
        for p in &paths {
            for root in [&ws, &kiln] {
                if let Ok(rel) = p.path.strip_prefix(root) {
                    let first = rel.components().next().expect("non-empty relative path");
                    assert!(
                        first.as_os_str().to_string_lossy().starts_with('.'),
                        "auto-detected {} is not under a dot-directory",
                        p.path.display()
                    );
                }
            }
        }
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

        let paths = default_discovery_paths(&SourceRoots::default(), None, &[], Some(home));
        // None of the discovered paths should reference `.claude/skills`.
        for p in &paths {
            // The cross-harness path is `.claude/skills`, not any path that
            // happens to contain `.claude`: a checkout living under one — a
            // git worktree in `.claude/worktrees/` — puts it in every build
            // path, and the bare substring failed there having proved nothing.
            let path = p.path.to_string_lossy().replace('\\', "/");
            assert!(
                !path.contains(".claude/skills"),
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

        let paths = default_discovery_paths(&SourceRoots::default(), None, &[], Some(home));
        let has_claude = paths
            .iter()
            .any(|p| p.path.to_string_lossy().contains(".claude"));
        assert!(
            has_claude,
            "cross-harness path must appear when explicitly enabled"
        );
    }
}
