use super::super::{IncludeError, merge_includes};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_merge_includes_gateway() {
    let temp = TempDir::new().unwrap();

    // Create the include file
    let mcps_content = r#"
[[servers]]
name = "github"
prefix = "gh_"

[servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
"#;
    fs::write(temp.path().join("mcps.toml"), mcps_content).unwrap();

    // Create main config with include
    let main_content = r#"
profile = "default"

[include]
gateway = "mcps.toml"

[cli]
verbose = true
"#;
    let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

    // Merge includes
    let result = merge_includes(&mut main_config, temp.path());
    assert!(result.is_ok(), "Merge should succeed");

    // Verify the gateway section was added
    let gateway = main_config
        .get("gateway")
        .expect("gateway section should exist");
    let servers = gateway.get("servers").expect("servers array should exist");
    assert!(servers.is_array());

    let servers_array = servers.as_array().unwrap();
    assert_eq!(servers_array.len(), 1);

    let first_server = &servers_array[0];
    assert_eq!(
        first_server.get("name").and_then(|v| v.as_str()),
        Some("github")
    );

    // Verify include section was removed
    assert!(main_config.get("include").is_none());

    // Verify other sections remain
    assert!(main_config.get("profile").is_some());
    assert!(main_config.get("cli").is_some());
}

#[test]
fn test_merge_includes_appends_arrays() {
    let temp = TempDir::new().unwrap();

    // Create include file with one server
    let include_content = r#"
[[servers]]
name = "included-server"

[servers.transport]
type = "stdio"
command = "included-cmd"
"#;
    fs::write(temp.path().join("extra.toml"), include_content).unwrap();

    // Main config already has a server
    let main_content = r#"
[include]
gateway = "extra.toml"

[[gateway.servers]]
name = "original-server"

[gateway.servers.transport]
type = "stdio"
command = "original-cmd"
"#;
    let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

    let result = merge_includes(&mut main_config, temp.path());
    assert!(result.is_ok());

    // Should have both servers
    let servers = main_config
        .get("gateway")
        .and_then(|g| g.get("servers"))
        .and_then(|s| s.as_array())
        .expect("servers array");

    assert_eq!(servers.len(), 2, "Should have original + included server");
}

#[test]
fn test_merge_includes_file_not_found() {
    let temp = TempDir::new().unwrap();

    let main_content = r#"
[include]
gateway = "nonexistent.toml"
"#;
    let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

    let result = merge_includes(&mut main_config, temp.path());
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], IncludeError::FileNotFound(_)));
}

#[test]
fn test_merge_includes_parse_error() {
    let temp = TempDir::new().unwrap();

    // Create invalid TOML
    fs::write(temp.path().join("bad.toml"), "invalid = [[[").unwrap();

    let main_content = r#"
[include]
gateway = "bad.toml"
"#;
    let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

    let result = merge_includes(&mut main_config, temp.path());
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], IncludeError::Parse { .. }));
}

#[test]
fn test_merge_includes_no_includes() {
    let main_content = r#"
profile = "default"

[cli]
verbose = true
"#;
    let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

    // Use temp dir even though it's not accessed (no includes to process)
    let base_dir = std::env::temp_dir().join("crucible_test_no_includes");
    let result = merge_includes(&mut main_config, &base_dir);
    assert!(result.is_ok());

    // Config should be unchanged
    assert!(main_config.get("profile").is_some());
    assert!(main_config.get("cli").is_some());
}
