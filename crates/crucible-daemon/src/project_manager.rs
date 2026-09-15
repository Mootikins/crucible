//! Project registration and management for the daemon.
//!
//! Projects are directories the user works on. This manager tracks
//! registered projects and provides CRUD operations. Projects are
//! persisted to a JSON file in the crucible home directory.

use crate::kiln_registry::KilnRegistry;
use crate::registry_store::RegistryStore;
use crucible_core::config::{read_kiln_config, read_project_config};
use crucible_core::{Project, ProjectKiln, RepositoryInfo};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// System trees that are never a project and always hold host secrets.
const SYSTEM_ROOTS: &[&str] = &["/etc", "/proc", "/sys", "/dev", "/boot", "/root", "/run"];

/// Directories that hold *every* user's home, matched EXACTLY — a path under
/// one of them is somebody's home, which is an ordinary place to keep work.
/// The home rule below already covers these for the user the daemon runs as;
/// this backstops the case where the OS reports no home directory at all,
/// where that rule silently does nothing.
const HOME_PARENTS: &[&str] = &["/home", "/Users"];

/// Why `path` may never be a root for ANY caller, or `None` if it may.
///
/// **This is the floor, not the whole policy.** It holds for every caller —
/// CLI, TUI, RPC, web — so it contains only the catastrophic set: roots that
/// enclose *every* user's data (`/`, `$HOME` and its ancestors, `/home`,
/// `/Users`) and the system trees that are never anybody's project. A local
/// user who runs `cru` inside their own dotfiles repo or `~/.config/nvim` is
/// registering their own work, and this must not stand in their way.
///
/// A registered root is also a read scope for clients (`/api/file/raw` serves
/// anything inside one), so *untrusted* callers need more than this floor.
/// That extra policy belongs where the untrusted caller is — see
/// `untrusted_root_refusal` in `crucible-web`'s project route, which refuses
/// credential stores and the user's config/state tree on top of this.
///
/// `path` must already be canonical: the decision is made on resolved paths so
/// a symlink cannot present an innocent name for a forbidden target. `home` is
/// injected rather than read from the environment so it is testable.
///
/// Returns the reason clause alone ("it is the filesystem root"); callers frame
/// it for the thing they were asked to do — a project root, a session kiln.
/// One project, from whichever layer declared it.
///
/// Not [`crucible_core::config::Registration`]: a project carries the kiln
/// NAMES it uses, and a kiln registration has nothing of the kind. The
/// precedence rule is shared through `overlay_layers`, which is generic
/// precisely so three registries of three shapes can state it once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLayerEntry {
    /// The name the project is addressed by.
    pub name: String,
    /// Its root. Absolute; comparison between layers is textual.
    pub path: std::path::PathBuf,
    /// The kiln names it uses, as registry names.
    pub kilns: Vec<String>,
    /// Which layer declared it.
    pub origin: crucible_core::config::RegistrationOrigin,
}

pub fn forbidden_root_reason(path: &Path, home: Option<&Path>) -> Option<&'static str> {
    if path.parent().is_none() {
        return Some("it is the filesystem root");
    }
    // `home.starts_with(path)` is true when `path` IS home or an ancestor of
    // it — every credential directory the user owns lives underneath.
    if home.is_some_and(|home| home.starts_with(path)) {
        return Some("it is the home directory or an ancestor of it, which would put every credential under it in scope");
    }
    if HOME_PARENTS.iter().any(|parent| path == Path::new(parent)) {
        return Some("it holds every user's home directory");
    }
    if SYSTEM_ROOTS.iter().any(|root| path.starts_with(root)) {
        return Some("it is inside a system directory");
    }

    None
}

/// The schema version this daemon writes and understands.
pub const PROJECT_STATE_VERSION: u32 = 1;

/// The whole of `projects.json`.
///
/// # Two shapes, one type
///
/// This file predates the state-store pattern and shipped as a bare JSON
/// array. Reading accepts both that and the versioned object; writing always
/// produces the versioned one, so a file upgrades itself the first time the
/// daemon touches it. A user who never registers another project keeps a
/// readable legacy file forever, which is the correct outcome — nothing is
/// rewritten for its own sake.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectStateFile {
    /// Written on every save so a future reader can refuse a file it is too
    /// old to model, rather than rewriting it through the wrong struct.
    pub version: u32,
    pub projects: Vec<Project>,
}

impl Default for ProjectStateFile {
    fn default() -> Self {
        Self {
            version: PROJECT_STATE_VERSION,
            projects: Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for ProjectStateFile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Untagged, and the object arm is first: a bare array cannot match the
        // struct, so a legacy file falls through to `Legacy` rather than
        // erroring. A legacy file is assigned the CURRENT version because it
        // holds exactly what this build models — there is nothing in it this
        // daemon could erase.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Shape {
            Versioned {
                version: u32,
                #[serde(default)]
                projects: Vec<Project>,
            },
            Legacy(Vec<Project>),
        }

        Ok(match Shape::deserialize(deserializer)? {
            Shape::Versioned { version, projects } => Self { version, projects },
            Shape::Legacy(projects) => Self {
                version: PROJECT_STATE_VERSION,
                projects,
            },
        })
    }
}

/// Refuse a file this daemon is too old to read.
///
/// Fail closed: a higher version means keys this build does not model, and
/// writing the file back would erase them.
fn gate_version(state: ProjectStateFile, path: &Path) -> anyhow::Result<ProjectStateFile> {
    anyhow::ensure!(
        state.version <= PROJECT_STATE_VERSION,
        "{} is version {}; this daemon understands version {PROJECT_STATE_VERSION}. \
         The daemon is older than the file — upgrade Crucible.",
        path.display(),
        state.version
    );
    Ok(state)
}

/// Manages registered projects in the daemon.
pub struct ProjectManager {
    projects: DashMap<PathBuf, Project>,
    /// The file, behind the same locked read-modify-write the kiln and LLM
    /// registries use.
    ///
    /// `projects.json` was the last registry writing itself: an unlocked
    /// `fs::write` of the in-memory map. Two writers interleaving lost an
    /// entry, a crash mid-write left a truncated file that `load` read back as
    /// the project list, and an older daemon silently erased keys a newer one
    /// wrote. Two of three registries being safe is worse than none, because
    /// nobody remembers which is which.
    store: RegistryStore<ProjectStateFile>,
    /// Which directories are kilns. A kiln root is never a project — see
    /// [`Self::kiln_root_refusal`]. `None` is a manager built before the
    /// registry exists (tests, the config migration) and then no directory
    /// is a kiln to it.
    kiln_registry: Option<Arc<KilnRegistry>>,
}

impl ProjectManager {
    pub fn new(storage_path: PathBuf) -> Self {
        let manager = Self {
            projects: DashMap::new(),
            store: RegistryStore::new(storage_path),
            kiln_registry: None,
        };
        if let Err(e) = manager.load() {
            warn!("Failed to load projects from storage: {}", e);
        }
        manager
    }

    /// Tell the manager which directories are kilns.
    ///
    /// Asked at every read and every registration rather than once at load:
    /// the registry is additive at runtime (`kiln.register`), so a directory
    /// can become a kiln after `projects.json` was read.
    pub fn with_kiln_registry(mut self, registry: Arc<KilnRegistry>) -> Self {
        self.kiln_registry = Some(registry);
        self
    }

    /// Why `path` is not a project, if it is a registered kiln root.
    ///
    /// A project is where work goes; a kiln is where knowledge goes. The two
    /// registries name directories independently, and nothing stopped one
    /// directory from being in both: a session created with a kiln as its
    /// workspace auto-registered the kiln, and the session rail then grouped
    /// sessions under a project header named after the kiln.
    ///
    /// `path` is compared through the registry's own reverse index, so both
    /// spellings of a directory (configured and symlink-resolved) match.
    fn kiln_root_refusal(&self, path: &Path) -> Option<ProjectError> {
        let name = self.kiln_registry.as_ref()?.name_for(path)?;
        Some(ProjectError::InvalidPath(format!(
            "{} is the root of the kiln '{name}', not a project: a kiln is where \
             knowledge goes, a project is where work goes",
            path.display()
        )))
    }

    /// Where the registry lives. Named in diagnostics.
    pub fn storage_path(&self) -> &Path {
        self.store.path()
    }

    /// Read, gate, mutate, write — all under one lock.
    ///
    /// Every writer goes through here, so the version gate cannot be skipped
    /// by a new call site, and no mutation can read the file, decide, and then
    /// write with another writer in between.
    fn update_file<R>(
        &self,
        mutate: impl FnOnce(&mut ProjectStateFile) -> R,
    ) -> Result<R, ProjectError> {
        let file = self.store.path().to_path_buf();
        self.store
            .update(|state| {
                *state = gate_version(std::mem::take(state), &file)?;
                Ok(mutate(state))
            })
            .map_err(|e| ProjectError::Storage(e.to_string()))
    }

    pub fn register(&self, path: &Path) -> Result<Project, ProjectError> {
        let canonical = path
            .canonicalize()
            .map_err(|_| ProjectError::InvalidPath(path.display().to_string()))?;

        if !canonical.is_dir() {
            return Err(ProjectError::InvalidPath(format!(
                "Not a directory: {}",
                canonical.display()
            )));
        }

        // `.crucible` directories are Crucible data/config dirs (kiln or
        // project metadata), never projects themselves. Registering one
        // produces nonsense like a project named ".crucible" with a nested
        // ".crucible/.crucible" kiln.
        if canonical.file_name().is_some_and(|n| n == ".crucible") {
            return Err(ProjectError::InvalidPath(format!(
                "{} is a Crucible data directory, not a project",
                canonical.display()
            )));
        }

        let repository = self.detect_repository(&canonical);

        // A project is the repo, not whichever subdirectory the CLI happened
        // to run from: registering from inside a repo resolves to the repo
        // root (worktrees resolve to their own worktree root). An explicit
        // `.crucible/project.toml` at the invocation dir opts a subdirectory
        // out and keeps it a project of its own.
        //
        // The root is re-canonicalized: a linked worktree's workdir comes from
        // a `gitdir` file whose absolute path need not be canonical, and both
        // the forbidden-root decision below and the `DashMap` key (which
        // `get`/`unregister` look up canonically) assume it is.
        let canonical = match repository.as_ref() {
            Some(repo)
                if repo.root != canonical
                    && repo.root.is_dir()
                    && read_project_config(&canonical).is_none() =>
            {
                debug!(
                    path = %canonical.display(),
                    root = %repo.root.display(),
                    "Resolving project registration to repository root"
                );
                repo.root.canonicalize().map_err(|_| {
                    ProjectError::InvalidPath(format!(
                        "Repository root does not resolve: {}",
                        repo.root.display()
                    ))
                })?
            }
            _ => canonical,
        };

        // Checked on the FINAL path: the repo-root resolution above can move a
        // registration upwards (a `<base>/x` inside a repo rooted at `$HOME`
        // would otherwise land on `$HOME`), so this has to see where it
        // actually ended up. `canonical` is a canonicalized path, so a symlink
        // cannot present an innocent name for a forbidden target.
        if let Some(why) = forbidden_root_reason(&canonical, dirs::home_dir().as_deref()) {
            return Err(ProjectError::ForbiddenRoot(format!(
                "{} is not a valid project root: {why}",
                canonical.display()
            )));
        }
        // Also on the FINAL path, and for the same reason: a kiln inside a
        // repository resolves to the repository, which is a project.
        if let Some(refusal) = self.kiln_root_refusal(&canonical) {
            return Err(refusal);
        }

        let (name, kilns) = self.read_project_metadata(&canonical);

        let mut project = Project::new(canonical.clone(), name).with_kilns(kilns);
        if let Some(repo) = repository {
            project = project.with_repository(repo);
        }

        // Written INSIDE the lock, against the file as it stands — not by
        // replacing the file with this process's in-memory map. The map is a
        // cache and can be stale; blasting it over the file is how a
        // concurrent registration disappears.
        self.update_file(|state| {
            state.projects.retain(|p| p.path != canonical);
            state.projects.push(project.clone());
        })?;
        self.projects.insert(canonical.clone(), project.clone());

        info!(
            path = %canonical.display(),
            name = %project.name,
            has_repo = project.repository.is_some(),
            "Project registered"
        );
        Ok(project)
    }

    pub fn register_if_missing(&self, path: &Path) -> Result<Project, ProjectError> {
        if let Some(existing) = self.get(path) {
            self.touch(path);
            return Ok(existing);
        }
        self.register(path)
    }

    /// Register every git repository that is a DIRECT child of `root` — the
    /// configured `[workspace] root_dir`. Returns how many NEW projects the
    /// scan added.
    ///
    /// The daemon runs this once at startup so a user who keeps every checkout
    /// under one directory sees them all in the web root picker without
    /// registering each by hand. Depth is exactly one: a repository nested
    /// deeper is a build artifact or a vendored tree far more often than it is
    /// work, and walking further would register hundreds of them.
    ///
    /// A missing `root` is the state before the first clone, so it is a no-op,
    /// not an error, and the directory is NOT created. A child that
    /// [`Self::register`] refuses (a forbidden root, an unresolvable symlink)
    /// is logged and skipped — one bad entry must not stop the scan.
    pub fn discover_repos_in(&self, root: &Path) -> usize {
        let Ok(entries) = fs::read_dir(root) else {
            debug!(root = %root.display(), "Workspace root dir is absent; nothing to discover");
            return 0;
        };

        let mut added = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            // `.git` is a directory in a normal checkout and a FILE in a linked
            // worktree or a submodule, so test for existence, not for a dir.
            if !path.is_dir() || !path.join(".git").exists() {
                continue;
            }
            if self.get(&path).is_some() {
                continue;
            }
            match self.register(&path) {
                Ok(project) => {
                    added += 1;
                    info!(path = %project.path.display(), "Discovered repository registered");
                }
                Err(e) => warn!(path = %path.display(), "Skipped discovered repository: {e}"),
            }
        }
        added
    }

    pub fn unregister(&self, path: &Path) -> Result<(), ProjectError> {
        let canonical = path
            .canonicalize()
            .map_err(|_| ProjectError::NotFound(path.to_path_buf()))?;

        // The FILE decides whether there was anything to remove, and it
        // decides under the lock. The in-memory map can be missing an entry a
        // second daemon wrote a moment ago, and reporting NotFound for one
        // that is on disk would leave it there forever.
        let removed = self.update_file(|state| {
            let before = state.projects.len();
            state.projects.retain(|p| p.path != canonical);
            state.projects.len() != before
        })?;

        self.projects.remove(&canonical);
        if !removed {
            return Err(ProjectError::NotFound(canonical));
        }

        info!(path = %canonical.display(), "Project unregistered");
        Ok(())
    }

    pub fn list(&self) -> Vec<Project> {
        let mut projects: Vec<Project> = self
            .projects
            .iter()
            .filter_map(|r| {
                let project = r.value().clone();
                // Filter out invalid entries
                if self.is_valid_project(&project) {
                    Some(project)
                } else {
                    None
                }
            })
            .collect();
        projects.sort_by_key(|p| std::cmp::Reverse(p.last_accessed));
        projects
    }

    /// Check if a project is valid for listing and lookup.
    /// Filters out:
    /// - Paths ending with `.crucible` (kiln subdirectories)
    /// - Non-existent paths
    /// - Registered kiln roots (see [`Self::kiln_root_refusal`]) — an entry
    ///   an older build wrote, or a directory that became a kiln after it
    ///   was registered. Decided at read, not dropped at load, because the
    ///   kiln registry grows while the daemon runs.
    fn is_valid_project(&self, project: &Project) -> bool {
        let path = &project.path;

        // Filter out paths ending with .crucible
        if path.ends_with(".crucible") {
            return false;
        }

        // Filter out non-existent paths
        if !path.exists() {
            return false;
        }

        if self.kiln_root_refusal(path).is_some() {
            debug!(path = %path.display(), "Registered project left out: it is a kiln root");
            return false;
        }

        true
    }

    /// The registered project at `path`, under the same rules as [`Self::list`]:
    /// an entry the list would leave out is not found here either, or
    /// `register_if_missing` would revive it with a touch.
    pub fn get(&self, path: &Path) -> Option<Project> {
        let canonical = path.canonicalize().ok()?;
        self.projects
            .get(&canonical)
            .map(|r| r.clone())
            .filter(|project| self.is_valid_project(project))
    }

    pub fn touch(&self, path: &Path) {
        let should_persist = if let Ok(canonical) = path.canonicalize() {
            if let Some(mut entry) = self.projects.get_mut(&canonical) {
                entry.touch();
                debug!(path = %canonical.display(), "Project touched");
                true
            } else {
                false
            }
            // `entry` guard is dropped here, releasing the shard lock
        } else {
            false
        };

        // Now safe to write — no DashMap locks held. The timestamp is applied
        // to the FILE's entry, not by writing this process's whole map over
        // it: a touch must not resurrect a project another writer just
        // unregistered, nor drop one it just added.
        if should_persist {
            if let Ok(canonical) = path.canonicalize() {
                let written = self.update_file(|state| {
                    match state.projects.iter_mut().find(|p| p.path == canonical) {
                        Some(project) => {
                            project.touch();
                            true
                        }
                        None => false,
                    }
                });
                match written {
                    Ok(true) => {}
                    Ok(false) => debug!(
                        path = %canonical.display(),
                        "Touched a project the registry file no longer holds"
                    ),
                    Err(e) => warn!("Failed to persist after touch: {}", e),
                }
            }
        }
    }

    fn detect_repository(&self, path: &Path) -> Option<RepositoryInfo> {
        match gix::discover(path) {
            Ok(repo) => {
                let git_dir = repo.git_dir().to_path_buf();
                let work_dir = repo.workdir().map(|p| p.to_path_buf());

                let common_dir = repo.common_dir().to_path_buf();
                let is_worktree = git_dir != common_dir;

                let root = work_dir.unwrap_or_else(|| git_dir.clone());

                let remote_url: Option<String> = repo
                    .find_default_remote(gix::remote::Direction::Fetch)
                    .and_then(|r| r.ok())
                    .and_then(|r: gix::Remote<'_>| {
                        r.url(gix::remote::Direction::Fetch)
                            .map(|u: &gix::Url| u.to_bstring().to_string())
                    });

                let main_repo_git_dir = if is_worktree { Some(common_dir) } else { None };

                debug!(
                    path = %path.display(),
                    root = %root.display(),
                    is_worktree,
                    remote = ?remote_url,
                    "Detected git repository"
                );

                Some(RepositoryInfo {
                    root,
                    remote_url,
                    is_worktree,
                    main_repo_git_dir,
                })
            }
            Err(_) => {
                debug!(path = %path.display(), "No git repository detected");
                None
            }
        }
    }

    fn read_project_metadata(&self, path: &Path) -> (String, Vec<ProjectKiln>) {
        // Try to read kiln config for the name
        let name = if let Some(kiln_config) = read_kiln_config(path) {
            kiln_config.kiln.name
        } else {
            // Fallback to directory name
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string()
        };

        // Try to read project config for kilns list
        let kilns = if let Some(project_config) = read_project_config(path) {
            project_config
                .kilns
                .into_iter()
                .map(|k| {
                    let kiln_path = if k.path.is_absolute() {
                        k.path
                    } else {
                        path.join(&k.path)
                    };
                    // Relative joins leave "./" segments in the stored path.
                    let kiln_path = kiln_path.canonicalize().unwrap_or(kiln_path);

                    ProjectKiln {
                        path: kiln_path,
                        name: k.name,
                    }
                })
                .collect()
        } else {
            // Fallback: a `.crucible` dir marks the PROJECT DIR as a kiln
            // root — the kiln path is the directory containing `.crucible`,
            // never the config dir itself (clients list notes/sessions from
            // the root; pointing them at `.crucible` yields empty trees).
            if path.join(".crucible").is_dir() {
                vec![ProjectKiln {
                    path: path.to_path_buf(),
                    name: None,
                }]
            } else {
                vec![]
            }
        };

        (name, kilns)
    }

    fn load(&self) -> Result<(), ProjectError> {
        let file = self.store.path().to_path_buf();
        let state = self
            .store
            .read()
            .and_then(|state| gate_version(state, &file))
            .map_err(|e| ProjectError::Storage(e.to_string()))?;
        let projects = state.projects;
        let home = dirs::home_dir();

        for mut project in projects {
            // Heal legacy entries: kiln paths used to be persisted as the
            // `.crucible` CONFIG dir instead of the kiln root, which gave
            // clients empty note trees. Normalize on load so old registry
            // files self-correct.
            for kiln in &mut project.kilns {
                if kiln.path.file_name().is_some_and(|n| n == ".crucible") {
                    if let Some(parent) = kiln.path.parent() {
                        kiln.path = parent.to_path_buf();
                    }
                }
                kiln.path = kiln
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| kiln.path.clone());
            }
            // Canonicalize before deciding: the file is on disk and editable,
            // and `forbidden_root_reason` only holds on resolved paths — a
            // persisted `/tmp/link` pointing at `/` would otherwise sail past
            // every rule. It also keeps the map key one `get`/`unregister` can
            // look up, since those canonicalize their argument.
            let Some(path) = project.path.canonicalize().ok().filter(|p| p.is_dir()) else {
                warn!(
                    path = %project.path.display(),
                    "Dropping stale project (directory missing)"
                );
                continue;
            };
            // Drop, don't keep: an entry like "/" can only have come from a
            // build that allowed it, and keeping it would leave the read scope
            // it granted alive across the upgrade. Loud, so the user can see
            // what changed and re-register something narrower.
            if let Some(why) = forbidden_root_reason(&path, home.as_deref()) {
                warn!(path = %path.display(), reason = why, "Dropping registered project with a forbidden root");
                continue;
            }
            project.path = path.clone();
            self.projects.insert(path, project);
        }

        debug!(
            path = %self.storage_path().display(),
            count = self.projects.len(),
            "Projects loaded"
        );
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("Project not found: {0}")]
    NotFound(PathBuf),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("{0}")]
    ForbiddenRoot(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The registry file could not be read or written. Distinct from `Io`
    /// because the message already names the file and says what to do about
    /// it — a version gate's refusal is not an errno.
    #[error("{0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_manager() -> (TempDir, ProjectManager) {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let manager = ProjectManager::new(storage);
        (tmp, manager)
    }

    #[test]
    fn register_and_list() {
        let (tmp, manager) = test_manager();
        let project_dir = tmp.path().join("my-project");
        fs::create_dir(&project_dir).unwrap();

        let project = manager.register(&project_dir).unwrap();
        assert_eq!(project.name, "my-project");
        assert_eq!(project.path, project_dir.canonicalize().unwrap());

        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "my-project");
    }

    #[test]
    fn register_with_workspace_config() {
        let (tmp, manager) = test_manager();
        let project_dir = tmp.path().join("configured-project");
        let crucible_dir = project_dir.join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();

        // Write kiln config with the name
        let kiln_config = r#"
[kiln]
name = "My Custom Name"
"#;
        fs::write(crucible_dir.join("kiln.toml"), kiln_config).unwrap();

        // Write project config with kilns list
        let project_config = r#"
[[kilns]]
path = "./notes"
"#;
        fs::write(crucible_dir.join("project.toml"), project_config).unwrap();

        let project = manager.register(&project_dir).unwrap();
        assert_eq!(project.name, "My Custom Name");
        assert_eq!(project.kilns.len(), 1);
        assert_eq!(project.kilns[0].name, None);
        assert_eq!(project.kilns[0].path, project_dir.join("notes"));
    }

    // ── A kiln root is never a project ──────────────────────────────────
    //
    // A project is where work goes; a kiln is where knowledge goes. The
    // session rail groups sessions under their project, so a kiln directory
    // that registered itself as a project (a session created with the kiln as
    // its workspace did exactly that) showed up as a project header named
    // after the kiln.

    /// A registry that names `kiln` — through the same floor as production.
    fn registry_naming(tmp: &TempDir, kiln: &Path) -> Arc<crate::kiln_registry::KilnRegistry> {
        crate::test_support::kiln_registry(&tmp.path().join("data"), &[("notes", kiln)])
    }

    #[test]
    fn a_registered_kiln_root_is_refused_as_a_project() {
        let tmp = TempDir::new().unwrap();
        let notes = tmp.path().join("notes");
        fs::create_dir(&notes).unwrap();
        let manager = ProjectManager::new(tmp.path().join("projects.json"))
            .with_kiln_registry(registry_naming(&tmp, &notes));

        let err = manager.register(&notes).unwrap_err();
        assert!(matches!(err, ProjectError::InvalidPath(_)), "{err:?}");
        assert!(err.to_string().contains("kiln"), "{err}");
        // The auto-registration door a session opens.
        assert!(manager.register_if_missing(&notes).is_err());
        assert!(manager.list().is_empty());
        assert!(manager.get(&notes).is_none());
    }

    /// The entry already on disk — written by a build that let a session's
    /// kiln-root workspace register itself — is left out rather than served.
    /// It is filtered at read, not dropped at load: the kiln registry grows at
    /// runtime (`kiln.register`), so a load-time decision is stale by the
    /// first registration.
    #[test]
    fn a_persisted_project_that_is_a_kiln_root_is_left_out() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let notes = tmp.path().join("notes");
        let work = tmp.path().join("work");
        fs::create_dir(&notes).unwrap();
        fs::create_dir(&work).unwrap();
        {
            let before = ProjectManager::new(storage.clone());
            before.register(&notes).unwrap();
            before.register(&work).unwrap();
            assert_eq!(before.list().len(), 2);
        }

        let manager =
            ProjectManager::new(storage).with_kiln_registry(registry_naming(&tmp, &notes));
        let listed: Vec<String> = manager.list().into_iter().map(|p| p.name).collect();
        assert_eq!(listed, vec!["work".to_string()]);
        assert!(manager.get(&notes).is_none());
        assert!(
            manager.register_if_missing(&notes).is_err(),
            "a session in the kiln must not revive the stale entry"
        );
        assert!(manager.get(&work).is_some());
    }

    /// A project that becomes a kiln root while the daemon runs leaves the
    /// list at once, without a restart.
    #[test]
    fn a_project_that_becomes_a_kiln_root_leaves_the_list() {
        let tmp = TempDir::new().unwrap();
        let notes = tmp.path().join("notes");
        fs::create_dir(&notes).unwrap();
        let registry = crate::test_support::kiln_registry(&tmp.path().join("data"), &[]);
        let manager = ProjectManager::new(tmp.path().join("projects.json"))
            .with_kiln_registry(registry.clone());
        manager.register(&notes).unwrap();
        assert_eq!(manager.list().len(), 1);

        registry
            .register_named(crate::test_support::kiln_name("notes"), &notes)
            .unwrap();
        assert!(manager.list().is_empty());
    }

    /// A kiln that lives inside a repository does not stop the repository
    /// from being the project: the registration resolves to the repo root, and
    /// only the FINAL path is judged. This is `~/crucible/docs` inside
    /// `~/crucible`, the shipped shape.
    #[test]
    fn a_kiln_inside_a_repository_still_registers_the_repository() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let docs = repo.join("docs");
        fs::create_dir_all(&docs).unwrap();
        gix::init(&repo).unwrap();
        let manager = ProjectManager::new(tmp.path().join("projects.json"))
            .with_kiln_registry(registry_naming(&tmp, &docs));

        let project = manager.register(&docs).unwrap();
        assert_eq!(project.path, repo.canonicalize().unwrap());
        assert_eq!(manager.list().len(), 1);
    }

    #[test]
    fn register_rejects_crucible_data_dirs() {
        let (tmp, manager) = test_manager();
        let data_dir = tmp.path().join(".crucible");
        fs::create_dir(&data_dir).unwrap();

        let err = manager.register(&data_dir).unwrap_err();
        assert!(matches!(err, ProjectError::InvalidPath(_)));
        assert!(manager.list().is_empty());
    }

    // ── Forbidden registration roots ────────────────────────────────────
    //
    // The floor holds for EVERY caller — CLI, RPC, and web alike — so it only
    // covers roots that are catastrophic for all of them: `/`, `$HOME` and its
    // ancestors, the dirs that hold every home, and the system trees. Anything
    // narrower (credential stores, the user's config tree) is per-client
    // policy and lives with the untrusted client; see the web project route.

    #[test]
    fn register_refuses_the_filesystem_root() {
        let (_tmp, manager) = test_manager();

        let err = manager.register(Path::new("/")).unwrap_err();
        assert!(matches!(err, ProjectError::ForbiddenRoot(_)), "{err:?}");
        assert!(manager.list().is_empty());
    }

    #[test]
    fn register_refuses_the_home_directory_and_every_ancestor_of_it() {
        let home = Path::new("/home/u");

        for path in ["/", "/home", "/home/u"] {
            assert!(
                forbidden_root_reason(Path::new(path), Some(home)).is_some(),
                "{path} must be refused"
            );
        }
        // A directory *under* home is a perfectly ordinary project.
        assert!(forbidden_root_reason(Path::new("/home/u/work/app"), Some(home)).is_none());
        // Another user's home is not an ancestor of ours — not this rule's job.
        assert!(forbidden_root_reason(Path::new("/home/other/app"), Some(home)).is_none());

        // The home rule is a no-op when the OS reports no home directory, so
        // the dirs that hold every home must be refused on their own.
        for path in ["/", "/home", "/Users"] {
            assert!(
                forbidden_root_reason(Path::new(path), None).is_some(),
                "{path} must be refused even with no home dir"
            );
        }
        assert!(forbidden_root_reason(Path::new("/home/u/work"), None).is_none());
    }

    #[test]
    fn register_refuses_system_directories() {
        let home = Path::new("/home/u");

        for path in [
            "/etc",
            "/etc/ssl/private",
            "/proc/self",
            "/root/x",
            "/run/x",
        ] {
            assert!(
                forbidden_root_reason(Path::new(path), Some(home)).is_some(),
                "{path} must be refused"
            );
        }
    }

    // Rewritten: these three cases used to be *refused* by the daemon floor
    // (`.ssh`/`.config` matched as a path component anywhere, plus a "holds a
    // credential store" probe). That refused ordinary local registrations —
    // a dotfiles repo, `~/.config/nvim` — for a threat model that only exists
    // on the web route. They are allowed here and refused there.
    #[test]
    fn register_allows_a_dotfiles_repo_that_holds_a_credential_store() {
        let (tmp, manager) = test_manager();
        let dotfiles = tmp.path().join("dotfiles");
        fs::create_dir_all(dotfiles.join(".ssh")).unwrap();
        fs::create_dir_all(dotfiles.join(".aws")).unwrap();

        let project = manager.register(&dotfiles).unwrap();
        assert_eq!(project.path, dotfiles.canonicalize().unwrap());
        assert_eq!(manager.list().len(), 1);
    }

    #[test]
    fn register_allows_a_project_inside_the_users_config_tree() {
        let home = Path::new("/home/u");

        // `~/.config/nvim` is somebody's editor config repo, and a `.config`
        // component ANYWHERE (`/home/u/work/app/.config`) is ordinary.
        for path in [
            "/home/u/.config/nvim",
            "/home/u/.local/share/chezmoi",
            "/home/u/work/app/.config",
            "/home/u/dotfiles/.ssh",
        ] {
            assert_eq!(
                forbidden_root_reason(Path::new(path), Some(home)),
                None,
                "{path} must be allowed for a local caller"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn register_refuses_a_symlink_that_resolves_to_a_forbidden_root() {
        let (tmp, manager) = test_manager();
        let link = tmp.path().join("innocent-looking");
        std::os::unix::fs::symlink("/", &link).unwrap();

        let err = manager.register(&link).unwrap_err();
        assert!(matches!(err, ProjectError::ForbiddenRoot(_)), "{err:?}");
        assert!(manager.list().is_empty());
    }

    #[test]
    fn load_drops_persisted_registrations_of_forbidden_roots() {
        // An entry written by an earlier (permissive) build must not keep
        // granting read scope after the upgrade — dropping it is the point.
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let good = tmp.path().join("keep-me");
        fs::create_dir(&good).unwrap();
        fs::write(
            &storage,
            serde_json::to_string(&[
                Project::new(PathBuf::from("/"), "root".to_string()),
                Project::new(good.canonicalize().unwrap(), "keep-me".to_string()),
            ])
            .unwrap(),
        )
        .unwrap();

        let manager = ProjectManager::new(storage);
        let list = manager.list();
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].name, "keep-me");
    }

    #[test]
    fn register_from_repo_subdir_resolves_to_repo_root() {
        let (tmp, manager) = test_manager();
        let repo = tmp.path().join("repo");
        let subdir = repo.join("crates").join("some-crate");
        fs::create_dir_all(&subdir).unwrap();
        gix::init(&repo).unwrap();

        let project = manager.register(&subdir).unwrap();
        assert_eq!(project.path, repo.canonicalize().unwrap());
        assert_eq!(project.name, "repo");
        assert_eq!(manager.list().len(), 1);
    }

    #[test]
    fn register_repo_subdir_with_own_project_config_stays_a_project() {
        let (tmp, manager) = test_manager();
        let repo = tmp.path().join("repo");
        let subdir = repo.join("standalone");
        let crucible_dir = subdir.join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        gix::init(&repo).unwrap();
        fs::write(
            crucible_dir.join("project.toml"),
            "[[kilns]]\npath = \"./notes\"\n",
        )
        .unwrap();

        let project = manager.register(&subdir).unwrap();
        assert_eq!(project.path, subdir.canonicalize().unwrap());
    }

    #[test]
    fn unregister() {
        let (tmp, manager) = test_manager();
        let project_dir = tmp.path().join("to-remove");
        fs::create_dir(&project_dir).unwrap();

        manager.register(&project_dir).unwrap();
        assert_eq!(manager.list().len(), 1);

        manager.unregister(&project_dir).unwrap();
        assert_eq!(manager.list().len(), 0);
    }

    #[test]
    fn get_project() {
        let (tmp, manager) = test_manager();
        let project_dir = tmp.path().join("get-test");
        fs::create_dir(&project_dir).unwrap();

        manager.register(&project_dir).unwrap();

        let project = manager.get(&project_dir).unwrap();
        assert_eq!(project.name, "get-test");

        assert!(manager.get(Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn persistence() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let project_dir = tmp.path().join("persist-test");
        fs::create_dir(&project_dir).unwrap();

        {
            let manager = ProjectManager::new(storage.clone());
            manager.register(&project_dir).unwrap();
            assert_eq!(manager.list().len(), 1);
        }

        {
            let manager = ProjectManager::new(storage);
            assert_eq!(manager.list().len(), 1);
            assert_eq!(manager.list()[0].name, "persist-test");
        }
    }

    /// Eight `ProjectManager`s over one file, registering at once.
    ///
    /// This is the property the unlocked writer did not have. It read the file
    /// (or rather, ignored it), serialized its own in-memory map, and wrote
    /// the lot — so two managers interleaving left only the entries the last
    /// one happened to hold. Each manager here knows about exactly one
    /// project, which is the worst case for that bug and a no-op for a writer
    /// that mutates the file under a lock.
    ///
    /// Separate managers, not threads sharing one: sharing a `DashMap` would
    /// hide the defect behind the in-memory merge.
    #[test]
    fn concurrent_writers_do_not_lose_a_project() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let dirs: Vec<PathBuf> = (0..8)
            .map(|n| {
                let dir = tmp.path().join(format!("project-{n}"));
                fs::create_dir(&dir).unwrap();
                dir
            })
            .collect();

        let handles: Vec<_> = dirs
            .iter()
            .map(|dir| {
                let storage = storage.clone();
                let dir = dir.clone();
                std::thread::spawn(move || {
                    ProjectManager::new(storage).register(&dir).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let reloaded = ProjectManager::new(storage);
        assert_eq!(
            reloaded.list().len(),
            8,
            "every writer's project must survive: {:?}",
            reloaded.list().iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }

    /// `projects.json` shipped as a bare JSON array. A file written by an
    /// older Crucible has to keep working, and the first write upgrades it.
    #[test]
    fn a_legacy_bare_array_file_is_read_and_upgraded_on_the_next_write() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let existing = tmp.path().join("existing");
        let added = tmp.path().join("added");
        fs::create_dir(&existing).unwrap();
        fs::create_dir(&added).unwrap();

        // The old shape, written by hand exactly as an older daemon wrote it.
        let legacy = serde_json::to_string_pretty(&vec![Project::new(
            existing.canonicalize().unwrap(),
            "existing".to_string(),
        )])
        .unwrap();
        fs::write(&storage, legacy).unwrap();

        let manager = ProjectManager::new(storage.clone());
        assert_eq!(manager.list().len(), 1, "the legacy file must be read");

        manager.register(&added).unwrap();

        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&storage).unwrap()).unwrap();
        assert_eq!(
            on_disk["version"], PROJECT_STATE_VERSION,
            "the first write upgrades the shape: {on_disk}"
        );
        assert_eq!(
            on_disk["projects"].as_array().map(Vec::len),
            Some(2),
            "and it keeps what the legacy file held: {on_disk}"
        );
    }

    /// Fail closed on a file from a newer Crucible. An older daemon writing it
    /// back through this build's struct would erase every key this build does
    /// not model.
    #[test]
    fn a_newer_file_is_refused_rather_than_rewritten() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let dir = tmp.path().join("project");
        fs::create_dir(&dir).unwrap();
        fs::write(
            &storage,
            r#"{"version": 99, "projects": [], "something_new": true}"#,
        )
        .unwrap();

        let manager = ProjectManager::new(storage.clone());
        let err = manager
            .register(&dir)
            .expect_err("a newer file must not be written through this struct");
        assert!(
            err.to_string().contains("projects.json"),
            "the refusal must name the file: {err}"
        );

        let after = fs::read_to_string(&storage).unwrap();
        assert!(
            after.contains("something_new"),
            "the file must be left exactly as it was: {after}"
        );
    }

    /// What `cru project forget` removes stays removed.
    ///
    /// The kiln registry has two layers — the config the user wrote out-ranks
    /// the state the daemon was told — so a kiln `forget` has to refuse a
    /// config-declared name or it would appear to work and be undone at the
    /// next boot. The project registry has NO config layer: `load` reads
    /// `projects.json` and nothing else. This test is what says so, because
    /// "there is no overlay" is invisible until someone adds one.
    #[test]
    fn a_forgotten_project_does_not_come_back_at_the_next_start() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let project_dir = tmp.path().join("forget-test");
        fs::create_dir(&project_dir).unwrap();

        {
            let manager = ProjectManager::new(storage.clone());
            manager.register(&project_dir).unwrap();
            manager.unregister(&project_dir).unwrap();
            assert!(manager.list().is_empty());
        }

        // A fresh process over the same file, which is where a config overlay
        // would put the entry back.
        let manager = ProjectManager::new(storage);
        assert!(
            manager.list().is_empty(),
            "a forgotten project must not be re-declared by any other layer"
        );
    }

    #[test]
    fn list_filters_nonexistent_paths() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let project_dir = tmp.path().join("valid-project");
        fs::create_dir(&project_dir).unwrap();

        let manager = ProjectManager::new(storage);
        manager.register(&project_dir).unwrap();
        assert_eq!(manager.list().len(), 1);

        fs::remove_dir(&project_dir).unwrap();
        assert_eq!(manager.list().len(), 0);
    }

    #[test]
    fn crucible_subdir_never_becomes_a_project() {
        let tmp = TempDir::new().unwrap();
        let storage = tmp.path().join("projects.json");
        let project_dir = tmp.path().join("my-project");
        let crucible_dir = project_dir.join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();

        let manager = ProjectManager::new(storage);
        manager.register(&project_dir).unwrap();
        assert_eq!(manager.list().len(), 1);

        // Registering the `.crucible` data dir is rejected outright, so the
        // list never grows a bogus ".crucible" project.
        let err = manager.register(&crucible_dir).unwrap_err();
        assert!(matches!(err, ProjectError::InvalidPath(_)));
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert!(!list[0].path.ends_with(".crucible"));
    }

    /// The startup scan registers every git repo that is a DIRECT child of the
    /// workspace root dir, and nothing else: a plain directory is not a repo, a
    /// repo one level deeper is not a direct child, and the root itself is never
    /// registered.
    #[test]
    fn discovery_registers_only_direct_child_repos() {
        let (tmp, manager) = test_manager();
        let root = tmp.path().join("Projects");

        let repo = root.join("a-repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(root.join("plain-dir")).unwrap();
        fs::create_dir_all(root.join("outer").join("nested-repo").join(".git")).unwrap();
        fs::write(root.join("a-file"), "x").unwrap();

        assert_eq!(manager.discover_repos_in(&root), 1);

        let paths: Vec<_> = manager.list().into_iter().map(|p| p.path).collect();
        assert_eq!(paths, vec![repo.canonicalize().unwrap()]);
    }

    /// The scan is idempotent and never resets `last_accessed` ordering: a
    /// second pass over the same root registers nothing new.
    #[test]
    fn discovery_is_idempotent() {
        let (tmp, manager) = test_manager();
        let root = tmp.path().join("Projects");
        fs::create_dir_all(root.join("a-repo").join(".git")).unwrap();

        assert_eq!(manager.discover_repos_in(&root), 1);
        assert_eq!(manager.discover_repos_in(&root), 0);
        assert_eq!(manager.list().len(), 1);
    }

    /// A missing root dir is the common case before the first clone. It must
    /// not be created, and must not be an error.
    #[test]
    fn discovery_tolerates_a_missing_root() {
        let (tmp, manager) = test_manager();
        let root = tmp.path().join("does-not-exist");

        assert_eq!(manager.discover_repos_in(&root), 0);
        assert!(!root.exists());
        assert_eq!(manager.list().len(), 0);
    }
}
