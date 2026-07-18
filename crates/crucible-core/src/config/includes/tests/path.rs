use super::super::resolve_include_path;
use crate::test_support::EnvVarGuard;
use std::path::PathBuf;

#[test]
fn test_resolve_include_path_relative() {
    let base = PathBuf::from("/home/user/.config/crucible");
    let resolved = resolve_include_path("mcps.toml", &base);
    assert_eq!(
        resolved,
        PathBuf::from("/home/user/.config/crucible/mcps.toml")
    );
}

#[test]
fn test_resolve_include_path_absolute() {
    let base = PathBuf::from("/home/user/.config/crucible");
    let resolved = resolve_include_path("/etc/crucible/mcps.toml", &base);
    assert_eq!(resolved, PathBuf::from("/etc/crucible/mcps.toml"));
}

#[test]
fn test_resolve_include_path_home() {
    // Pin HOME so `~` expansion is deterministic and the assertion always runs
    // (previously it silently asserted nothing when HOME was unset). This is a
    // pure path computation — no filesystem access — so a fixed value is fine.
    let _home = EnvVarGuard::set("HOME", "/home/testuser".to_string());
    let base = PathBuf::from("/some/path");
    let resolved = resolve_include_path("~/crucible/mcps.toml", &base);

    let home = dirs::home_dir().expect("HOME is pinned");
    assert_eq!(resolved, home.join("crucible/mcps.toml"));
}
