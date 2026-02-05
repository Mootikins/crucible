//! Project registration and management for the daemon.
//!
//! Projects are directories the user works on. This manager tracks
//! registered projects and provides CRUD operations. Projects are
//! persisted to a JSON file in the crucible home directory.

use crucible_config::WorkspaceConfig;
use crucible_core::{Project, RepositoryInfo};
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

        let (name, kilns) = self.read_project_metadata(&canonical);
        let repository = self.detect_repository(&canonical);

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

    pub fn find_by_repository(&self, repo_root: &Path) -> Vec<Project> {
        self.projects
            .iter()
            .filter(|entry| {
                entry.value().repository.as_ref().map_or(false, |r| {
                    let id = r.main_repo_git_dir.as_ref().unwrap_or(&r.root);
                    id == repo_root
                })
            })
            .map(|entry| entry.value().clone())
            .collect()
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
        let mut projects: Vec<Project> = self.projects.iter().map(|r| r.value().clone()).collect();
        projects.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        projects
    }

    pub fn get(&self, path: &Path) -> Option<Project> {
        let canonical = path.canonicalize().ok()?;
        self.projects.get(&canonical).map(|r| r.clone())
    }

    pub fn touch(&self, path: &Path) {
        if let Ok(canonical) = path.canonicalize() {
            if let Some(mut entry) = self.projects.get_mut(&canonical) {
                entry.touch();
                debug!(path = %canonical.display(), "Project touched");
                if let Err(e) = self.persist() {
                    warn!("Failed to persist after touch: {}", e);
                }
            }
        }
    }

    fn detect_repository(&self, path: &Path) -> Option<RepositoryInfo> {
        match gix::discover(path) {
            Ok(repo) => {
                let git_dir = repo.git_dir().to_path_buf();
                let work_dir = repo.work_dir().map(|p| p.to_path_buf());

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

    fn read_project_metadata(&self, path: &Path) -> (String, Vec<PathBuf>) {
        let config_path = path.join(".crucible").join("workspace.toml");
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(config) = toml::from_str::<WorkspaceConfig>(&content) {
                let name = config.workspace.name;
                let kilns: Vec<PathBuf> = config
                    .kilns
                    .into_iter()
                    .map(|k| {
                        if k.path.is_absolute() {
                            k.path
                        } else {
                            path.join(&k.path)
                        }
                    })
                    .collect();
                return (name, kilns);
            }
        }

        let crucible_dir = path.join(".crucible");
        let kilns = if crucible_dir.is_dir() {
            vec![crucible_dir]
        } else {
            vec![]
        };

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

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

        for project in projects {
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

        let config = r#"
[workspace]
name = "My Custom Name"

[[kilns]]
path = "./notes"
"#;
        fs::write(crucible_dir.join("workspace.toml"), config).unwrap();

        let project = manager.register(&project_dir).unwrap();
        assert_eq!(project.name, "My Custom Name");
        assert_eq!(project.kilns.len(), 1);
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
}
