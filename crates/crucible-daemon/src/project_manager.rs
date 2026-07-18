//! Project registration and management for the daemon.
//!
//! Projects are directories the user works on. This manager tracks
//! registered projects and provides CRUD operations. Projects are
//! persisted to a JSON file in the crucible home directory.

use crucible_core::config::{read_kiln_config, read_project_config};
use crucible_core::{Project, ProjectKiln, RepositoryInfo};
use dashmap::DashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Manages registered projects in the daemon.
pub struct ProjectManager {
    projects: DashMap<PathBuf, Project>,
    storage_path: PathBuf,
}

impl ProjectManager {
    pub fn new(storage_path: PathBuf) -> Self {
        let manager = Self {
            projects: DashMap::new(),
            storage_path,
        };
        if let Err(e) = manager.load() {
            warn!("Failed to load projects from storage: {}", e);
        }
        manager
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
                repo.root.clone()
            }
            _ => canonical,
        };

        let (name, kilns) = self.read_project_metadata(&canonical);

        let mut project = Project::new(canonical.clone(), name).with_kilns(kilns);
        if let Some(repo) = repository {
            project = project.with_repository(repo);
        }

        self.projects.insert(canonical.clone(), project.clone());
        self.persist()?;

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

    pub fn unregister(&self, path: &Path) -> Result<(), ProjectError> {
        let canonical = path
            .canonicalize()
            .map_err(|_| ProjectError::NotFound(path.to_path_buf()))?;

        if self.projects.remove(&canonical).is_none() {
            return Err(ProjectError::NotFound(canonical));
        }

        self.persist()?;
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
        projects.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        projects
    }

    /// Check if a project is valid for listing.
    /// Filters out:
    /// - Paths ending with `.crucible` (kiln subdirectories)
    /// - Non-existent paths
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

        true
    }

    pub fn get(&self, path: &Path) -> Option<Project> {
        let canonical = path.canonicalize().ok()?;
        self.projects.get(&canonical).map(|r| r.clone())
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

        // Now safe to call persist() - no DashMap locks held
        if should_persist {
            if let Err(e) = self.persist() {
                warn!("Failed to persist after touch: {}", e);
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

    fn persist(&self) -> Result<(), ProjectError> {
        let projects: Vec<Project> = self.projects.iter().map(|r| r.value().clone()).collect();
        let json = serde_json::to_string_pretty(&projects)?;

        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.storage_path, json)?;
        debug!(path = %self.storage_path.display(), count = projects.len(), "Projects persisted");
        Ok(())
    }

    fn load(&self) -> Result<(), ProjectError> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&self.storage_path)?;
        let projects: Vec<Project> = serde_json::from_str(&content)?;

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
            if project.path.is_dir() {
                self.projects.insert(project.path.clone(), project);
            } else {
                warn!(
                    path = %project.path.display(),
                    "Dropping stale project (directory missing)"
                );
            }
        }

        debug!(
            path = %self.storage_path.display(),
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

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
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

    #[test]
    fn register_rejects_crucible_data_dirs() {
        let (tmp, manager) = test_manager();
        let data_dir = tmp.path().join(".crucible");
        fs::create_dir(&data_dir).unwrap();

        let err = manager.register(&data_dir).unwrap_err();
        assert!(matches!(err, ProjectError::InvalidPath(_)));
        assert!(manager.list().is_empty());
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
}
