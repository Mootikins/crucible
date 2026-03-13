//! Tests for configuration types.

use super::*;
use crate::components::{ChatConfig, DiscoveryPathsConfig, GatewayConfig, HandlersConfig};
use crucible_core::test_support::EnvVarGuard;
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
fn test_config_with_new_sections() {
    let toml_content = r#"
profile = "default"

[discovery.type_configs.tools]
additional_paths = ["/custom/tools"]
use_defaults = true

[[gateway.servers]]
name = "github"
prefix = "gh_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[handlers.builtin.test_filter]
enabled = true
pattern = "just_test*"
priority = 10

[handlers.builtin.tool_selector]
enabled = true
allowed_tools = ["search_*"]
"#;

    let config: Config = toml::from_str(toml_content).unwrap();

    // Check discovery config
    assert!(config.discovery.is_some());
    let discovery = config.discovery.as_ref().unwrap();
    assert!(discovery.type_configs.contains_key("tools"));

    // Check gateway config
    assert!(config.gateway.is_some());
    let gateway = config.gateway.as_ref().unwrap();
    assert_eq!(gateway.servers.len(), 1);
    assert_eq!(gateway.servers[0].name, "github");

    // Check handlers config
    assert!(config.handlers.is_some());
    let handlers = config.handlers.as_ref().unwrap();
    assert!(handlers.builtin.test_filter.enabled);
    assert!(handlers.builtin.tool_selector.enabled);
}

#[test]
fn test_validate_gateway_empty_name() {
    let config = Config {
        gateway: Some(GatewayConfig {
            servers: vec![crate::components::gateway::UpstreamServerConfig {
                name: "".to_string(),
                transport: crate::components::gateway::TransportType::Stdio {
                    command: "test".to_string(),
                    args: vec![],
                    env: std::collections::HashMap::new(),
                },
                prefix: None,
                allowed_tools: None,
                blocked_tools: None,
                auto_reconnect: true,
            }],
        }),
        ..Config::default()
    };

    let result = config.validate_gateway();
    assert!(result.is_err());
}

#[test]
fn test_validate_gateway_invalid_sse_url() {
    let config = Config {
        gateway: Some(GatewayConfig {
            servers: vec![crate::components::gateway::UpstreamServerConfig {
                name: "test".to_string(),
                transport: crate::components::gateway::TransportType::Sse {
                    url: "invalid-url".to_string(),
                    auth_header: None,
                },
                prefix: None,
                allowed_tools: None,
                blocked_tools: None,
                auto_reconnect: true,
            }],
        }),
        ..Config::default()
    };

    let result = config.validate_gateway();
    assert!(result.is_err());
}

#[test]
fn test_validate_gateway_valid() {
    let config = Config {
        gateway: Some(GatewayConfig {
            servers: vec![crate::components::gateway::UpstreamServerConfig {
                name: "test".to_string(),
                transport: crate::components::gateway::TransportType::Sse {
                    url: "http://localhost:3000/sse".to_string(),
                    auth_header: None,
                },
                prefix: Some("test_".to_string()),
                allowed_tools: None,
                blocked_tools: None,
                auto_reconnect: true,
            }],
        }),
        ..Config::default()
    };

    let result = config.validate_gateway();
    assert!(result.is_ok());
}

#[test]
fn test_validate_handlers_empty_pattern() {
    let config = Config {
        handlers: Some(HandlersConfig {
            builtin: crate::components::BuiltinHandlersTomlConfig {
                test_filter: crate::components::HandlerConfig {
                    enabled: true,
                    pattern: Some("".to_string()),
                    priority: Some(10),
                },
                ..Default::default()
            },
        }),
        ..Config::default()
    };

    let result = config.validate_handlers();
    assert!(result.is_err());
}

#[test]
fn test_validate_handlers_valid() {
    let config = Config {
        handlers: Some(HandlersConfig {
            builtin: crate::components::BuiltinHandlersTomlConfig {
                test_filter: crate::components::HandlerConfig {
                    enabled: true,
                    pattern: Some("just_test*".to_string()),
                    priority: Some(10),
                },
                ..Default::default()
            },
        }),
        ..Config::default()
    };

    let result = config.validate_handlers();
    assert!(result.is_ok());
}

#[test]
fn test_validate_all_sections() {
    let config = Config {
        gateway: Some(GatewayConfig {
            servers: vec![crate::components::gateway::UpstreamServerConfig {
                name: "test".to_string(),
                transport: crate::components::gateway::TransportType::Stdio {
                    command: "npx".to_string(),
                    args: vec![],
                    env: std::collections::HashMap::new(),
                },
                prefix: None,
                allowed_tools: None,
                blocked_tools: None,
                auto_reconnect: true,
            }],
        }),
        handlers: Some(HandlersConfig::default()),
        ..Config::default()
    };

    let result = config.validate();
    assert!(result.is_ok());
}

#[test]
fn test_validate_discovery_empty_path() {
    use std::collections::HashMap;
    let mut type_configs = HashMap::new();
    type_configs.insert(
        "tools".to_string(),
        crate::components::TypeDiscoveryConfig {
            additional_paths: vec![std::path::PathBuf::from("")],
            use_defaults: true,
        },
    );

    let config = Config {
        discovery: Some(crate::components::DiscoveryPathsConfig {
            handlers: None,
            tools: None,
            events: None,
            type_configs,
        }),
        ..Config::default()
    };

    let result = config.validate_discovery();
    assert!(result.is_err());
}

#[test]
fn test_validate_discovery_valid() {
    use std::collections::HashMap;
    let mut type_configs = HashMap::new();
    type_configs.insert(
        "tools".to_string(),
        crate::components::TypeDiscoveryConfig {
            additional_paths: vec![std::path::PathBuf::from("/valid/path")],
            use_defaults: true,
        },
    );

    let config = Config {
        discovery: Some(crate::components::DiscoveryPathsConfig {
            handlers: None,
            tools: None,
            events: None,
            type_configs,
        }),
        ..Config::default()
    };

    let result = config.validate_discovery();
    assert!(result.is_ok());
}

#[test]
fn test_config_default_has_new_sections_none() {
    let config = Config::default();
    assert!(config.discovery.is_none());
    assert!(config.gateway.is_none());
    assert!(config.handlers.is_none());
}

#[test]
fn test_config_accessor_methods() {
    let config = Config {
        discovery: Some(DiscoveryPathsConfig::default()),
        gateway: Some(GatewayConfig::default()),
        handlers: Some(HandlersConfig::default()),
        ..Config::default()
    };

    assert!(config.discovery_config().is_some());
    assert!(config.gateway_config().is_some());
    assert!(config.handlers_config().is_some());
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
fn test_effective_llm_provider_from_llm_config() {
    use std::collections::HashMap;
    let mut providers = HashMap::new();
    providers.insert(
        "local".to_string(),
        crate::components::LlmProviderConfig {
            provider_type: crate::components::BackendType::Ollama,
            endpoint: Some("http://192.168.1.100:11434".to_string()),
            default_model: Some("llama3.1:70b".to_string()),
            temperature: Some(0.9),
            max_tokens: Some(8192),
            timeout_secs: Some(300),
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    let config = Config {
        llm: Some(crate::components::LlmConfig {
            default: Some("local".to_string()),
            providers,
        }),
        ..Config::default()
    };

    let effective = config.effective_llm_provider().unwrap();
    assert_eq!(effective.key, "local");
    assert_eq!(effective.endpoint, "http://192.168.1.100:11434");
    assert_eq!(effective.model, "llama3.1:70b");
    assert_eq!(effective.temperature, 0.9);
    assert_eq!(effective.max_tokens, 8192);
    assert_eq!(effective.timeout_secs, 300);
}

#[test]
fn test_effective_llm_provider_without_llm_default_returns_error() {
    let config = Config {
        llm: None,
        chat: Some(ChatConfig {
            model: Some("gpt-4o".to_string()),
            enable_markdown: true,
            agent_preference: crate::components::AgentPreference::default(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            temperature: Some(0.8),
            max_tokens: Some(4096),
            timeout_secs: Some(60),
            size_aware_prompts: true,
            show_thinking: false,
        }),
        ..Config::default()
    };

    let effective = config.effective_llm_provider();
    assert!(effective.is_err());
}

#[test]
fn test_config_with_llm_section_from_toml() {
    let toml_content = r#"
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
endpoint = "http://localhost:11434"
default_model = "llama3.2"
temperature = 0.7
timeout_secs = 120

[llm.providers.cloud]
type = "openai"
api_key = "OPENAI_API_KEY"
default_model = "gpt-4o"
temperature = 0.7
max_tokens = 4096
"#;

    let config: Config = toml::from_str(toml_content).unwrap();

    assert!(config.llm.is_some());
    let llm = config.llm.as_ref().unwrap();
    assert_eq!(llm.default, Some("local".to_string()));
    assert_eq!(llm.providers.len(), 2);

    let local = llm.get_provider("local").unwrap();
    assert_eq!(local.provider_type, crate::components::BackendType::Ollama);
    assert_eq!(local.model(), "llama3.2");

    let cloud = llm.get_provider("cloud").unwrap();
    assert_eq!(cloud.provider_type, crate::components::BackendType::OpenAI);
    assert_eq!(cloud.model(), "gpt-4o");
    assert_eq!(cloud.api_key, Some("OPENAI_API_KEY".to_string()));
}

#[test]
fn test_cli_app_config_effective_llm_provider() {
    use std::collections::HashMap;
    let mut providers = HashMap::new();
    providers.insert(
        "local".to_string(),
        crate::components::LlmProviderConfig {
            provider_type: crate::components::BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    let config = CliAppConfig {
        llm: crate::components::LlmConfig {
            default: Some("local".to_string()),
            providers,
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
        llm: crate::components::LlmConfig::default(),
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
        crate::components::BackendType::Ollama
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
fn include_config_default_all_none() {
    let inc = crate::includes::IncludeConfig::default();
    assert!(
        inc.is_empty(),
        "default IncludeConfig should have is_empty() == true"
    );
}

#[test]
fn include_config_deser_without_database_field() {
    // IncludeConfig has no `database` field; TOML with only `gateway` must succeed.
    let toml_content = r#"
gateway = "mcps.toml"
"#;
    let inc: crate::includes::IncludeConfig = toml::from_str(toml_content).unwrap();
    assert_eq!(inc.gateway.as_deref(), Some("mcps.toml"));
    assert!(!inc.is_empty());
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
