use super::super::{process_file_references, IncludeError, ResolveMode};
use crate::test_support::EnvVarGuard;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_file_ref_string_value() {
    let temp = TempDir::new().unwrap();

    // Create a plain text file (like a secret key)
    fs::write(temp.path().join("api.key"), "sk-secret-key-12345\n").unwrap();

    let config_content = r#"
[embedding]
provider = "openai"
api_key = "{file:api.key}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    // The api_key should now be the file content (trimmed)
    let api_key = config
        .get("embedding")
        .and_then(|e| e.get("api_key"))
        .and_then(|k| k.as_str())
        .expect("api_key should exist");

    assert_eq!(api_key, "sk-secret-key-12345");
}

#[test]
fn test_file_ref_toml_value() {
    let temp = TempDir::new().unwrap();

    // Create a TOML file to include
    let gateway_content = r#"
[[servers]]
name = "github"
prefix = "gh_"

[servers.transport]
type = "stdio"
command = "npx"
"#;
    fs::write(temp.path().join("gateway.toml"), gateway_content).unwrap();

    let config_content = r#"
profile = "default"
gateway = "{file:gateway.toml}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    // The gateway should now be the parsed TOML content
    let gateway = config.get("gateway").expect("gateway should exist");
    assert!(gateway.is_table());

    let servers = gateway.get("servers").expect("servers should exist");
    assert!(servers.is_array());

    let first_server = servers.as_array().unwrap().first().unwrap();
    assert_eq!(
        first_server.get("name").and_then(|n| n.as_str()),
        Some("github")
    );
}

#[test]
fn test_file_ref_in_array() {
    let temp = TempDir::new().unwrap();

    // Create files with paths
    fs::write(temp.path().join("path1.txt"), "/opt/tools").unwrap();
    fs::write(temp.path().join("path2.txt"), "/usr/local/tools").unwrap();

    let config_content = r#"
extra_paths = ["{file:path1.txt}", "{file:path2.txt}", "/static/path"]
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    let paths = config
        .get("extra_paths")
        .and_then(|p| p.as_array())
        .expect("extra_paths should be an array");

    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0].as_str(), Some("/opt/tools"));
    assert_eq!(paths[1].as_str(), Some("/usr/local/tools"));
    assert_eq!(paths[2].as_str(), Some("/static/path"));
}

#[test]
fn test_file_ref_nested() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("secret.txt"), "super-secret").unwrap();

    let config_content = r#"
[level1]
[level1.level2]
[level1.level2.level3]
secret = "{file:secret.txt}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_ok());

    let secret = config
        .get("level1")
        .and_then(|l1| l1.get("level2"))
        .and_then(|l2| l2.get("level3"))
        .and_then(|l3| l3.get("secret"))
        .and_then(|s| s.as_str())
        .expect("secret should exist");

    assert_eq!(secret, "super-secret");
}

#[test]
fn test_file_ref_not_found() {
    let temp = TempDir::new().unwrap();

    let config_content = r#"
api_key = "{file:nonexistent.key}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], IncludeError::FileNotFound(_)));
}

#[test]
fn test_file_ref_with_home_path() {
    // This test just verifies the path is resolved correctly
    // (actual file won't exist, so we check the error path)
    let temp = TempDir::new().unwrap();
    // Pin HOME to the (empty) TempDir so `~/.secrets/test.key` deterministically
    // does NOT exist — a real ~/.secrets/test.key on the dev machine would
    // otherwise resolve and flip this test. EnvVarGuard restores HOME on drop.
    let _home = EnvVarGuard::set("HOME", temp.path().to_string_lossy().into_owned());

    let config_content = r#"
api_key = "{file:~/.secrets/test.key}"
"#;
    let mut config: toml::Value = toml::from_str(config_content).unwrap();

    let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
    // Should fail with FileNotFound (not a parse error)
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(matches!(errors[0], IncludeError::FileNotFound(_)));

    // Verify the path was resolved to home directory
    if let IncludeError::FileNotFound(path) = &errors[0] {
        let home = dirs::home_dir().expect("HOME is pinned to the TempDir");
        assert!(path.starts_with(home));
    }
}
