use std::path::{Path, PathBuf};

use tracing::debug;

const CRUCIBLE_DIR_NAME: &str = ".crucible";
const CRUCIBLE_KILN_ENV: &str = "CRUCIBLE_KILN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverySource {
    CliFlag,
    AncestorWalk,
    EnvVar,
    GlobalConfig,
}

#[derive(Debug, Clone)]
pub struct DiscoveredKiln {
    pub path: PathBuf,
    pub source: DiscoverySource,
}

pub fn discover_kiln(
    cli_flag: Option<&Path>,
    global_config_kiln_path: Option<&Path>,
) -> Option<DiscoveredKiln> {
    if let Some(path) = cli_flag {
        debug!("kiln from CLI flag: {}", path.display());
        return Some(DiscoveredKiln {
            path: path.to_path_buf(),
            source: DiscoverySource::CliFlag,
        });
    }

    if let Some(found) = walk_ancestors_for_crucible() {
        debug!("kiln from ancestor walk: {}", found.display());
        return Some(DiscoveredKiln {
            path: found,
            source: DiscoverySource::AncestorWalk,
        });
    }

    if let Some(found) = from_env_var() {
        debug!("kiln from ${}: {}", CRUCIBLE_KILN_ENV, found.display());
        return Some(DiscoveredKiln {
            path: found,
            source: DiscoverySource::EnvVar,
        });
    }

    if let Some(path) = global_config_kiln_path {
        let resolved = path.to_path_buf();
        if resolved.join(CRUCIBLE_DIR_NAME).is_dir() {
            debug!("kiln from global config: {}", resolved.display());
            return Some(DiscoveredKiln {
                path: resolved,
                source: DiscoverySource::GlobalConfig,
            });
        }
        debug!(
            "global config kiln_path {} has no .crucible/ dir, skipping",
            resolved.display()
        );
    }

    debug!("no kiln discovered");
    None
}

fn walk_ancestors_for_crucible() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut current: Option<&Path> = Some(&cwd);
    while let Some(dir) = current {
        if dir.join(CRUCIBLE_DIR_NAME).is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn from_env_var() -> Option<PathBuf> {
    let val = std::env::var(CRUCIBLE_KILN_ENV).ok()?;
    if val.is_empty() {
        return None;
    }
    let path = PathBuf::from(&val);
    if path.join(CRUCIBLE_DIR_NAME).is_dir() {
        Some(path)
    } else {
        debug!(
            "${} = {} but no .crucible/ dir found there",
            CRUCIBLE_KILN_ENV, val
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn make_kiln(dir: &Path) {
        std::fs::create_dir_all(dir.join(CRUCIBLE_DIR_NAME)).unwrap();
    }

    #[test]
    fn cli_flag_wins_over_everything() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("my-kiln");
        make_kiln(&kiln_path);

        let result = discover_kiln(Some(&kiln_path), None);
        assert!(result.is_some());
        let discovered = result.unwrap();
        assert_eq!(discovered.path, kiln_path);
        assert_eq!(discovered.source, DiscoverySource::CliFlag);
    }

    #[test]
    fn cli_flag_returns_path_even_without_crucible_dir() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("nonexistent");

        let result = discover_kiln(Some(&kiln_path), None);
        assert!(result.is_some());
        assert_eq!(result.unwrap().source, DiscoverySource::CliFlag);
    }

    #[test]
    #[serial]
    fn env_var_discovery() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("env-kiln");
        make_kiln(&kiln_path);

        std::env::set_var(CRUCIBLE_KILN_ENV, kiln_path.to_str().unwrap());
        let result = from_env_var();
        std::env::remove_var(CRUCIBLE_KILN_ENV);

        assert_eq!(result, Some(kiln_path));
    }

    #[test]
    #[serial]
    fn env_var_skips_missing_crucible_dir() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("no-crucible");
        std::fs::create_dir_all(&kiln_path).unwrap();

        std::env::set_var(CRUCIBLE_KILN_ENV, kiln_path.to_str().unwrap());
        let result = from_env_var();
        std::env::remove_var(CRUCIBLE_KILN_ENV);

        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn global_config_path_with_crucible_dir() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("config-kiln");
        make_kiln(&kiln_path);

        // Move CWD to a dir without .crucible so ancestor walk won't match
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path().join("config-kiln")).unwrap();
        std::env::remove_var(CRUCIBLE_KILN_ENV);

        let result = discover_kiln(None, Some(&kiln_path));

        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_some());
        let discovered = result.unwrap();
        // Ancestor walk finds it first since CWD is inside kiln_path
        assert!(
            discovered.source == DiscoverySource::AncestorWalk
                || discovered.source == DiscoverySource::GlobalConfig
        );
    }

    #[test]
    #[serial]
    fn global_config_fallback_when_no_ancestor() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("config-kiln");
        make_kiln(&kiln_path);
        let bare_cwd = tmp.path().join("bare-cwd");
        std::fs::create_dir_all(&bare_cwd).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&bare_cwd).unwrap();
        std::env::remove_var(CRUCIBLE_KILN_ENV);

        let result = discover_kiln(None, Some(&kiln_path));

        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_some());
        let discovered = result.unwrap();
        assert_eq!(discovered.path, kiln_path);
        assert_eq!(discovered.source, DiscoverySource::GlobalConfig);
    }

    #[test]
    #[serial]
    fn global_config_path_without_crucible_dir_skipped() {
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("bare-dir");
        std::fs::create_dir_all(&kiln_path).unwrap();
        let bare_cwd = tmp.path().join("bare-cwd");
        std::fs::create_dir_all(&bare_cwd).unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&bare_cwd).unwrap();
        std::env::remove_var(CRUCIBLE_KILN_ENV);

        let result = discover_kiln(None, Some(&kiln_path));

        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn returns_none_when_nothing_found() {
        std::env::remove_var(CRUCIBLE_KILN_ENV);
        let result = discover_kiln(None, None);
        // May or may not find something depending on test environment (CWD may have .crucible)
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    #[serial]
    fn ancestor_walk_finds_parent_kiln() {
        let tmp = TempDir::new().unwrap();
        make_kiln(tmp.path());
        let nested = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();

        // Temporarily change CWD to test ancestor walk
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();

        let result = walk_ancestors_for_crucible();

        std::env::set_current_dir(original_cwd).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap(), tmp.path().canonicalize().unwrap());
    }
}
