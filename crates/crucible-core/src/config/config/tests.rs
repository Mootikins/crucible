//! Tests for configuration types.

use super::*;
use crate::test_support::EnvVarGuard;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

/// Cross-platform test path helper
fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}

#[test]
fn test_crucible_home_and_is_crucible_home() {
    // Test env override
    let tmp = std::env::temp_dir().join("crucible_test_home_combined");
    let _guard = EnvVarGuard::set("CRUCIBLE_HOME", tmp.to_string_lossy().to_string());
    assert_eq!(crucible_home(), tmp);
    assert!(is_crucible_home(&tmp));
    assert!(!is_crucible_home(std::path::Path::new("/some/other/path")));
}

#[test]
fn test_agent_directories_default_empty() {
    let config = CliAppConfig::default();
    assert!(config.agent_directories.is_empty());
}

#[test]
fn test_agent_directories_loads_from_toml() {
    let kiln_path = test_path("test-kiln");
    let toml_content = format!(
        r#"
kiln_path = "{}"
agent_directories = ["/home/user/shared-agents", "./local-agents"]
"#,
        kiln_path.to_string_lossy().replace('\\', "\\\\")
    );
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(toml_content.as_bytes()).unwrap();

    let config = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None).unwrap();

    assert_eq!(config.agent_directories.len(), 2);
    assert_eq!(
        config.agent_directories[0],
        std::path::PathBuf::from("/home/user/shared-agents")
    );
    assert_eq!(
        config.agent_directories[1],
        std::path::PathBuf::from("./local-agents")
    );
}

#[test]
fn test_agent_directories_optional_when_missing() {
    let kiln_path = test_path("test-kiln");
    let toml_content = format!(
        r#"
kiln_path = "{}"
"#,
        kiln_path.to_string_lossy().replace('\\', "\\\\")
    );
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(toml_content.as_bytes()).unwrap();

    let config = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None).unwrap();

    assert!(config.agent_directories.is_empty());
}

#[test]
fn test_server_config_default_sets_auto_archive_hours() {
    let server = ServerConfig::default();
    assert_eq!(server.auto_archive_hours, Some(72));
}

#[test]
fn test_server_config_deserializes_auto_archive_hours() {
    let parsed: ServerConfig = toml::from_str(
        r#"
host = "127.0.0.1"
port = 8080
auto_archive_hours = 24
"#,
    )
    .unwrap();

    assert_eq!(parsed.auto_archive_hours, Some(24));
}

#[test]
fn test_cli_app_config_effective_llm_provider() {
    use std::collections::HashMap;
    let mut providers = HashMap::new();
    providers.insert(
        "local".to_string(),
        crate::config::components::LlmProviderConfig {
            provider_type: crate::config::components::BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
            name: None,
        },
    );

    let config = CliAppConfig {
        llm: crate::config::components::LlmConfig {
            default: Some("local".to_string()),
            providers,
            models: Default::default(),
        },
        ..Default::default()
    };

    let effective = config.effective_llm_provider().unwrap();
    assert_eq!(effective.key, "local");
    assert_eq!(effective.model, "llama3.2");
    assert_eq!(effective.temperature, 0.7);
}

#[test]
fn test_cli_app_config_effective_llm_provider_missing_default_errors() {
    let config = CliAppConfig::default();
    let effective = config.effective_llm_provider();
    assert!(effective.is_err());
}

#[test]
fn test_effective_llm_provider_requires_llm_default_provider() {
    let config = CliAppConfig {
        llm: crate::config::components::LlmConfig::default(),
        ..Default::default()
    };

    let effective = config.effective_llm_provider();
    assert!(
        effective.is_err(),
        "effective_llm_provider should fail without llm.default"
    );
}

#[test]
fn test_cli_app_config_rejects_legacy_embedding_section() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[embedding]
provider = "fastembed"
"#;
    std::fs::write(temp.path(), toml_content).unwrap();

    let parsed = CliAppConfig::load(Some(temp.path().to_path_buf()), None, None);
    assert!(
        parsed.is_err(),
        "legacy [embedding] config should be rejected"
    );
}

#[test]
fn test_cli_app_config_rejects_legacy_providers_section() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[providers]
default_embedding = "legacy"

[providers.legacy]
backend = "ollama"
"#;
    std::fs::write(temp.path(), toml_content).unwrap();

    let parsed = CliAppConfig::load(Some(temp.path().to_path_buf()), None, None);
    assert!(
        parsed.is_err(),
        "legacy [providers] config should be rejected"
    );
}

#[test]
fn test_cli_app_config_loads_llm_provider_config() {
    let kiln_path = test_path("test");
    let toml = format!(
        r#"
kiln_path = "{}"

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"
"#,
        kiln_path.to_string_lossy().replace('\\', "\\\\")
    );
    let config: CliAppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.llm.default, Some("local".to_string()));
    let provider = config.llm.providers.get("local").unwrap();
    assert_eq!(
        provider.provider_type,
        crate::config::components::BackendType::Ollama
    );
    assert_eq!(provider.model(), "llama3.2");
}

#[test]
fn test_cli_app_config_rejects_chat_provider_field() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let toml_content = r#"
kiln_path = "/tmp/test-kiln"

[chat]
provider = "openai"
"#;
    std::fs::write(temp.path(), toml_content).unwrap();

    let parsed = CliAppConfig::load(Some(temp.path().to_path_buf()), None, None);
    assert!(parsed.is_err(), "chat.provider should be rejected");
}

// ---- Golden regression tests ----

#[test]
fn database_path_derived_from_kiln() {
    let config = CliAppConfig {
        kiln_path: PathBuf::from("/tmp/test"),
        ..Default::default()
    };
    let db_path = config.database_path();
    assert!(
        db_path.starts_with("/tmp/test/.crucible"),
        "database path should be under kiln/.crucible, got: {}",
        db_path.display()
    );
    let filename = db_path.file_name().unwrap().to_string_lossy();
    assert!(
        filename.starts_with("crucible") && filename.ends_with(".db"),
        "database file should be crucible*.db, got: {}",
        filename
    );
}

#[test]
fn database_path_str_is_valid_utf8() {
    let config = CliAppConfig {
        kiln_path: PathBuf::from("/tmp/test"),
        ..Default::default()
    };
    let result = config.database_path_str();
    assert!(
        result.is_ok(),
        "database_path_str should return Ok for ASCII path"
    );
}

#[test]
fn logging_level_returns_none_when_unset() {
    let config = CliAppConfig::default();
    assert_eq!(
        config.logging_level(),
        None,
        "default config should have no logging level"
    );
}

// ---- PluginsConfig tests ----

#[test]
fn plugins_config_deserializes_from_toml() {
    let toml_content = r#"
[[plugin]]
url = "https://github.com/user/my-plugin.git"
branch = "main"

[[plugin]]
url = "other-user/other-plugin"
pin = "v1.0.0"
enabled = false
"#;
    let config: crate::config::config::PluginsConfig = toml::from_str(toml_content).unwrap();
    assert_eq!(config.plugin.len(), 2);

    assert_eq!(
        config.plugin[0].url,
        "https://github.com/user/my-plugin.git"
    );
    assert_eq!(config.plugin[0].branch.as_deref(), Some("main"));
    assert!(config.plugin[0].pin.is_none());
    assert!(config.plugin[0].enabled); // default_true

    assert_eq!(config.plugin[1].url, "other-user/other-plugin");
    assert_eq!(config.plugin[1].pin.as_deref(), Some("v1.0.0"));
    assert!(!config.plugin[1].enabled);
}

#[test]
fn plugins_config_empty_deserializes() {
    let config: crate::config::config::PluginsConfig = toml::from_str("").unwrap();
    assert!(config.plugin.is_empty());
}

#[test]
fn plugins_config_default_enabled_is_true() {
    let toml_content = r#"
[[plugin]]
url = "user/repo"
"#;
    let config: crate::config::config::PluginsConfig = toml::from_str(toml_content).unwrap();
    assert!(config.plugin[0].enabled);
}

#[test]
fn plugins_config_roundtrips_through_toml() {
    let config = crate::config::config::PluginsConfig {
        plugin: vec![crate::config::config::PluginEntry {
            url: "user/my-plugin".to_string(),
            branch: Some("dev".to_string()),
            pin: None,
            enabled: true,
        }],
    };
    let serialized = toml::to_string_pretty(&config).unwrap();
    let deserialized: crate::config::config::PluginsConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.plugin.len(), 1);
    assert_eq!(deserialized.plugin[0].url, "user/my-plugin");
    assert_eq!(deserialized.plugin[0].branch.as_deref(), Some("dev"));
}
