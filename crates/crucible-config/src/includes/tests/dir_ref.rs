use super::super::{IncludeError, ResolveMode, process_file_references};
use crucible_core::test_support::EnvVarGuard;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_dir_ref_merges_toml_files() {
    let temp = TempDir::new().unwrap();

    // Create a directory with config fragments
    let providers_dir = temp.path().join("providers.d");
    fs::create_dir(&providers_dir).unwrap();

    // Files are sorted alphabetically, so use numeric prefixes
    fs::write(
        providers_dir.join("00-local.toml"),
        r#"
[local]
backend = "ollama"
endpoint = "http://localhost:11434"
"#,
    )
    .unwrap();

    fs::write(
        providers_dir.join("10-cloud.toml"),
        r#"
[cloud]
backend = "openai"
api_key = "sk-test"
"#,
    )
    .unwrap();

    let config_content = r#"
providers = "{dir:providers.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok(), "Should succeed: {:?}", result);

    // Should have merged both files
    let providers = config.get("providers").expect("providers should exist");
    assert!(providers.is_table());

    let local = providers.get("local").expect("local should exist");
    assert_eq!(local.get("backend").unwrap().as_str(), Some("ollama"));

    let cloud = providers.get("cloud").expect("cloud should exist");
    assert_eq!(cloud.get("backend").unwrap().as_str(), Some("openai"));
}

#[test]
fn test_dir_ref_sorted_order() {
    let temp = TempDir::new().unwrap();

    let conf_dir = temp.path().join("conf.d");
    fs::create_dir(&conf_dir).unwrap();

    // Same key in multiple files - later files should override
    fs::write(
        conf_dir.join("00-base.toml"),
        r#"
name = "base"
timeout = 30
"#,
    )
    .unwrap();

    fs::write(
        conf_dir.join("99-override.toml"),
        r#"
name = "override"
"#,
    )
    .unwrap();

    let config_content = r#"
settings = "{dir:conf.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    let settings = config.get("settings").unwrap();
    // 99-override.toml should override 00-base.toml
    assert_eq!(settings.get("name").unwrap().as_str(), Some("override"));
    // But timeout from 00-base.toml should remain
    assert_eq!(settings.get("timeout").unwrap().as_integer(), Some(30));
}

#[test]
fn test_dir_ref_ignores_non_toml() {
    let temp = TempDir::new().unwrap();

    let conf_dir = temp.path().join("conf.d");
    fs::create_dir(&conf_dir).unwrap();

    fs::write(
        conf_dir.join("config.toml"),
        r#"
key = "value"
"#,
    )
    .unwrap();

    // These should be ignored
    fs::write(conf_dir.join("README.md"), "# Documentation").unwrap();
    fs::write(conf_dir.join(".hidden"), "hidden file").unwrap();
    fs::write(conf_dir.join("backup.toml.bak"), "backup").unwrap();

    let config_content = r#"
settings = "{dir:conf.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    let settings = config.get("settings").unwrap();
    assert_eq!(settings.get("key").unwrap().as_str(), Some("value"));
    // Only 1 key from the one .toml file
    assert_eq!(settings.as_table().unwrap().len(), 1);
}

#[test]
fn test_dir_ref_empty_directory() {
    let temp = TempDir::new().unwrap();

    let empty_dir = temp.path().join("empty.d");
    fs::create_dir(&empty_dir).unwrap();

    let config_content = r#"
settings = "{dir:empty.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    // Should be an empty table
    let settings = config.get("settings").unwrap();
    assert!(settings.is_table());
    assert!(settings.as_table().unwrap().is_empty());
}

#[test]
fn test_dir_ref_not_found() {
    let temp = TempDir::new().unwrap();

    let config_content = r#"
settings = "{dir:nonexistent.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(matches!(errors[0], IncludeError::DirNotFound(_)));
}

#[test]
fn test_dir_ref_with_nested_refs() {
    let temp = TempDir::new().unwrap();

    // Set up env var for nested ref
    let _guard = EnvVarGuard::set("CRUCIBLE_TEST_DIR_KEY", "nested-secret".to_string());

    let conf_dir = temp.path().join("conf.d");
    fs::create_dir(&conf_dir).unwrap();

    // File with {env:} reference inside
    fs::write(
        conf_dir.join("secrets.toml"),
        r#"
api_key = "{env:CRUCIBLE_TEST_DIR_KEY}"
"#,
    )
    .unwrap();

    let config_content = r#"
settings = "{dir:conf.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    let settings = config.get("settings").unwrap();
    assert_eq!(
        settings.get("api_key").unwrap().as_str(),
        Some("nested-secret")
    );
}

#[test]
fn test_dir_ref_with_home_path() {
    // Test that ~ paths are resolved (will fail with DirNotFound since dir doesn't exist)
    let temp = TempDir::new().unwrap();

    let config_content = r#"
settings = "{dir:~/.config/crucible/nonexistent.d/}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(matches!(errors[0], IncludeError::DirNotFound(_)));

    // Verify path was resolved to home directory
    if let IncludeError::DirNotFound(path) = &errors[0] {
        if let Some(home) = dirs::home_dir() {
            assert!(path.starts_with(home), "Path should start with home dir");
        }
    }
}

#[test]
fn test_dir_ref_ignores_subdirectories() {
    let temp = TempDir::new().unwrap();

    let conf_dir = temp.path().join("conf.d");
    fs::create_dir(&conf_dir).unwrap();

    // Create a toml file
    fs::write(
        conf_dir.join("config.toml"),
        r#"
key = "value"
"#,
    )
    .unwrap();

    // Create a subdirectory with toml files (should be ignored)
    let sub_dir = conf_dir.join("subdir");
    fs::create_dir(&sub_dir).unwrap();
    fs::write(
        sub_dir.join("nested.toml"),
        r#"
nested_key = "nested_value"
"#,
    )
    .unwrap();

    let config_content = r#"
settings = "{dir:conf.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    let settings = config.get("settings").unwrap();
    // Should only have the top-level key, not nested
    assert_eq!(settings.get("key").unwrap().as_str(), Some("value"));
    assert!(
        settings.get("nested_key").is_none(),
        "Subdirs should be ignored"
    );
}

#[test]
fn test_dir_ref_parse_error_continues() {
    let temp = TempDir::new().unwrap();

    let conf_dir = temp.path().join("conf.d");
    fs::create_dir(&conf_dir).unwrap();

    // Valid file
    fs::write(
        conf_dir.join("00-valid.toml"),
        r#"
valid_key = "valid_value"
"#,
    )
    .unwrap();

    // Invalid TOML file
    fs::write(conf_dir.join("50-invalid.toml"), "invalid = [[[").unwrap();

    // Another valid file
    fs::write(
        conf_dir.join("99-also-valid.toml"),
        r#"
another_key = "another_value"
"#,
    )
    .unwrap();

    let config_content = r#"
settings = "{dir:conf.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    // Should have errors from the invalid file
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], IncludeError::Parse { .. }));
}

#[test]
fn test_dir_ref_deep_merge_tables() {
    let temp = TempDir::new().unwrap();

    let conf_dir = temp.path().join("conf.d");
    fs::create_dir(&conf_dir).unwrap();

    // First file with nested table
    fs::write(
        conf_dir.join("00-base.toml"),
        r#"
[server]
host = "localhost"
port = 8080

[server.tls]
enabled = false
"#,
    )
    .unwrap();

    // Second file adds to nested table
    fs::write(
        conf_dir.join("10-tls.toml"),
        r#"
[server.tls]
enabled = true
cert = "/path/to/cert.pem"
"#,
    )
    .unwrap();

    let config_content = r#"
settings = "{dir:conf.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok(), "Should succeed: {:?}", result);

    let settings = config.get("settings").unwrap();
    let server = settings.get("server").unwrap();

    // Original values preserved
    assert_eq!(server.get("host").unwrap().as_str(), Some("localhost"));
    assert_eq!(server.get("port").unwrap().as_integer(), Some(8080));

    // TLS section was deep-merged
    let tls = server.get("tls").unwrap();
    assert_eq!(tls.get("enabled").unwrap().as_bool(), Some(true)); // Overridden
    assert_eq!(tls.get("cert").unwrap().as_str(), Some("/path/to/cert.pem"));
    // Added
}

#[test]
fn test_dir_ref_appends_arrays() {
    let temp = TempDir::new().unwrap();

    let conf_dir = temp.path().join("mcps.d");
    fs::create_dir(&conf_dir).unwrap();

    // First file with servers array
    fs::write(
        conf_dir.join("00-github.toml"),
        r#"
[[servers]]
name = "github"
prefix = "gh_"
"#,
    )
    .unwrap();

    // Second file adds more servers
    fs::write(
        conf_dir.join("10-gitlab.toml"),
        r#"
[[servers]]
name = "gitlab"
prefix = "gl_"
"#,
    )
    .unwrap();

    let config_content = r#"
gateway = "{dir:mcps.d}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok(), "Should succeed: {:?}", result);

    let gateway = config.get("gateway").unwrap();
    let servers = gateway.get("servers").unwrap().as_array().unwrap();

    // Both servers should be present (arrays appended)
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0].get("name").unwrap().as_str(), Some("github"));
    assert_eq!(servers[1].get("name").unwrap().as_str(), Some("gitlab"));
}
