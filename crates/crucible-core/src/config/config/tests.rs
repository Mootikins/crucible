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
fn test_crucible_home_env_override() {
    let tmp = std::env::temp_dir().join("crucible_test_home_combined");
    let _guard = EnvVarGuard::set("CRUCIBLE_HOME", tmp.to_string_lossy().to_string());
    assert_eq!(crucible_home(), tmp);
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
    assert_eq!(server.auto_archive_hours, 72);
}

#[test]
fn test_server_config_deserializes_auto_archive_hours() {
    let parsed: ServerConfig = toml::from_str(
        r#"
auto_archive_hours = 24
"#,
    )
    .unwrap();

    assert_eq!(parsed.auto_archive_hours, 24);
}

/// A retired `[server]` key fails loudly and names itself.
///
/// `host`, `port`, `https`, `cert_file`, `key_file`, `max_body_size` and
/// `timeout_seconds` were removed because nothing read them — the daemon binds
/// a Unix socket and the web server's address comes from `[web]`.
/// `deny_unknown_fields` stays on this struct on purpose: a config that still
/// sets one gets an error naming the key, which is a one-line fix, rather than
/// silent acceptance of a setting that does nothing.
#[test]
fn a_retired_server_key_is_rejected_by_name() {
    let err = toml::from_str::<ServerConfig>("port = 8080\n")
        .expect_err("a removed key must not deserialize");
    let msg = err.to_string();
    assert!(msg.contains("port"), "the error must name the key: {msg}");
}

#[test]
fn test_cli_app_config_effective_llm_provider() {
    use std::collections::BTreeMap;
    let mut providers = BTreeMap::new();
    providers.insert(
        "local".to_string(),
        crate::config::components::LlmProviderConfig {
            provider_type: crate::config::components::BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
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
    assert_eq!(effective.model, "llama3.2");
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

/// A `[discovery]` section in an old config file does not stop the load.
///
/// Plan item T5-09 deleted the unread `DiscoveryPathsConfig` types. The root
/// config does not set `deny_unknown_fields`, so the section is ignored.
#[test]
fn a_discovery_section_is_ignored_not_rejected() {
    let config: CliAppConfig = toml::from_str(
        r#"
[discovery.tools]
additional_paths = ["/opt/crucible/tools"]
use_defaults = false
"#,
    )
    .expect("an ignored [discovery] section must still load");
    assert_eq!(config.kiln_path, CliAppConfig::default().kiln_path);
}
