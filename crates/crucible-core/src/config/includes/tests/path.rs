use super::super::resolve_include_path;
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
    let base = PathBuf::from("/some/path");
    let resolved = resolve_include_path("~/crucible/mcps.toml", &base);

    // Should start with home directory
    if let Some(home) = dirs::home_dir() {
        assert_eq!(resolved, home.join("crucible/mcps.toml"));
    }
}
