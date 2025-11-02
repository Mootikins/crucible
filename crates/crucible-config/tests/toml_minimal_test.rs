//! Minimal TOML serialization/deserialization tests
//!
//! This test file progressively tests TOML parsing from simple structs
//! to nested structures to identify exactly where parsing fails.

use crucible_config::{
    ApiConfig, Config, EmbeddingProviderConfig, EmbeddingProviderType, ModelConfig,
};
use std::collections::HashMap;

#[test]
fn test_api_config_serialization() {
    let api = ApiConfig {
        key: None,
        base_url: Some("local".to_string()),
        timeout_seconds: Some(60),
        retry_attempts: Some(1),
        headers: HashMap::new(),
    };

    // Serialize to TOML
    let toml_str = toml::to_string_pretty(&api).expect("Failed to serialize ApiConfig");
    println!("=== ApiConfig TOML ===\n{}\n", toml_str);

    // Deserialize back
    let deserialized: ApiConfig = toml::from_str(&toml_str).expect("Failed to deserialize ApiConfig");
    assert_eq!(api.base_url, deserialized.base_url);
    assert_eq!(api.timeout_seconds, deserialized.timeout_seconds);
}

#[test]
fn test_model_config_serialization() {
    let model = ModelConfig {
        name: "nomic-embed-text-v1.5".to_string(),
        dimensions: Some(768),
        max_tokens: Some(512),
    };

    // Serialize to TOML
    let toml_str = toml::to_string_pretty(&model).expect("Failed to serialize ModelConfig");
    println!("=== ModelConfig TOML ===\n{}\n", toml_str);

    // Deserialize back
    let deserialized: ModelConfig = toml::from_str(&toml_str).expect("Failed to deserialize ModelConfig");
    assert_eq!(model.name, deserialized.name);
    assert_eq!(model.dimensions, deserialized.dimensions);
}

#[test]
fn test_embedding_provider_config_serialization() {
    let provider = EmbeddingProviderConfig {
        provider_type: EmbeddingProviderType::FastEmbed,
        api: ApiConfig {
            key: None,
            base_url: Some("local".to_string()),
            timeout_seconds: Some(60),
            retry_attempts: Some(1),
            headers: HashMap::new(),
        },
        model: ModelConfig {
            name: "nomic-embed-text-v1.5".to_string(),
            dimensions: Some(768),
            max_tokens: Some(512),
        },
        options: HashMap::new(),
    };

    // Serialize to TOML
    let toml_str = toml::to_string_pretty(&provider).expect("Failed to serialize EmbeddingProviderConfig");
    println!("=== EmbeddingProviderConfig TOML ===\n{}\n", toml_str);

    // Deserialize back
    let deserialized: EmbeddingProviderConfig =
        toml::from_str(&toml_str).expect("Failed to deserialize EmbeddingProviderConfig");
    assert_eq!(provider.provider_type, deserialized.provider_type);
    assert_eq!(provider.model.name, deserialized.model.name);
}

#[test]
fn test_embedding_provider_config_from_nested_tables() {
    // This is the format we WANT to support
    let toml_nested = r#"
type = "fastembed"

[api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1

[model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512
"#;

    println!("=== Attempting to parse nested table format ===\n{}\n", toml_nested);

    let result: Result<EmbeddingProviderConfig, _> = toml::from_str(toml_nested);
    match result {
        Ok(config) => {
            println!("✓ Successfully parsed nested tables!");
            assert_eq!(config.provider_type, EmbeddingProviderType::FastEmbed);
            assert_eq!(config.model.name, "nomic-embed-text-v1.5");
        }
        Err(e) => {
            println!("✗ Failed to parse nested tables:");
            println!("{}", e);
            panic!("Nested table parsing failed: {}", e);
        }
    }
}

#[test]
fn test_embedding_provider_config_from_inline_tables() {
    // This is the inline table format
    let toml_inline = r#"
type = "fastembed"
api = { base_url = "local", timeout_seconds = 60, retry_attempts = 1 }
model = { name = "nomic-embed-text-v1.5", dimensions = 768, max_tokens = 512 }
"#;

    println!("=== Attempting to parse inline table format ===\n{}\n", toml_inline);

    let result: Result<EmbeddingProviderConfig, _> = toml::from_str(toml_inline);
    match result {
        Ok(config) => {
            println!("✓ Successfully parsed inline tables!");
            assert_eq!(config.provider_type, EmbeddingProviderType::FastEmbed);
            assert_eq!(config.model.name, "nomic-embed-text-v1.5");
        }
        Err(e) => {
            println!("✗ Failed to parse inline tables:");
            println!("{}", e);
            panic!("Inline table parsing failed: {}", e);
        }
    }
}

#[test]
fn test_full_config_with_embedding_nested_tables() {
    // This is the ACTUAL format from the user's config file
    let toml_config = r#"
profile = "default"

[profiles.default]
name = "default"
environment = "test"

[embedding]
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1
"#;

    println!("=== Attempting to parse full Config with [embedding.model] nested tables ===\n{}\n", toml_config);

    let result: Result<Config, _> = toml::from_str(toml_config);
    match result {
        Ok(config) => {
            println!("✓ Successfully parsed full Config with nested tables!");
            assert!(config.embedding.is_some());
            let embedding = config.embedding.unwrap();
            assert_eq!(embedding.provider_type, EmbeddingProviderType::FastEmbed);
            assert_eq!(embedding.model.name, "nomic-embed-text-v1.5");
            assert_eq!(embedding.model.dimensions, Some(768));
        }
        Err(e) => {
            println!("✗ Failed to parse full Config:");
            println!("{}", e);
            panic!("Full Config parsing failed: {}", e);
        }
    }
}

#[test]
fn test_serialize_full_config() {
    let embedding = EmbeddingProviderConfig {
        provider_type: EmbeddingProviderType::FastEmbed,
        api: ApiConfig {
            key: None,
            base_url: Some("local".to_string()),
            timeout_seconds: Some(60),
            retry_attempts: Some(1),
            headers: HashMap::new(),
        },
        model: ModelConfig {
            name: "nomic-embed-text-v1.5".to_string(),
            dimensions: Some(768),
            max_tokens: Some(512),
        },
        options: HashMap::new(),
    };

    let mut config = Config::new();
    config.embedding = Some(embedding);

    // Serialize to TOML
    let toml_str = toml::to_string_pretty(&config).expect("Failed to serialize Config");
    println!("=== Full Config TOML ===\n{}\n", toml_str);

    // Deserialize back
    let deserialized: Config = toml::from_str(&toml_str).expect("Failed to deserialize Config");
    assert!(deserialized.embedding.is_some());
}

#[test]
fn test_actual_user_config_file() {
    // This is the ACTUAL user's config file format
    let toml_config = r#"
# Crucible CLI Configuration
# Location: ~/.config/crucible/config.toml

[kiln]
# Path to your Obsidian kiln
# Default: current directory
path = "/home/moot/Documents/crucible-testing"

[embedding]
# Embedding provider configuration (FastEmbed - local, no server needed)
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1
"#;

    println!("=== Attempting to parse ACTUAL user config ===\n{}\n", toml_config);

    let result: Result<Config, _> = toml::from_str(toml_config);
    match result {
        Ok(config) => {
            println!("✓ Successfully parsed user config!");
            assert!(config.embedding.is_some());
            let embedding = config.embedding.unwrap();
            assert_eq!(embedding.provider_type, EmbeddingProviderType::FastEmbed);
            assert_eq!(embedding.model.name, "nomic-embed-text-v1.5");
        }
        Err(e) => {
            println!("✗ Failed to parse user config:");
            println!("{}", e);
            panic!("User config parsing failed: {}", e);
        }
    }
}

/// Regression test for the flatten attribute issue
/// This test ensures that Config can be deserialized with nested tables
/// even when profiles are present (which previously had flatten on settings)
#[test]
fn test_regression_flatten_with_profiles_and_nested_tables() {
    // This is the exact scenario that failed before the fix:
    // - Config has profiles
    // - Profiles had #[serde(flatten)] on settings (FIXED)
    // - Embedding has nested tables like [embedding.model]
    let toml_config = r#"
profile = "default"

[profiles.default]
name = "default"
environment = "test"

[embedding]
type = "fastembed"

[embedding.model]
name = "nomic-embed-text-v1.5"
dimensions = 768
max_tokens = 512

[embedding.api]
base_url = "local"
timeout_seconds = 60
retry_attempts = 1
"#;

    println!("=== Regression test: profiles + nested embedding tables ===\n{}\n", toml_config);

    let result: Result<Config, _> = toml::from_str(toml_config);
    match result {
        Ok(config) => {
            println!("✓ Regression test passed!");

            // Verify profiles work
            assert!(config.profiles.contains_key("default"));
            assert_eq!(config.profile, Some("default".to_string()));

            // Verify nested embedding tables work
            assert!(config.embedding.is_some());
            let embedding = config.embedding.unwrap();
            assert_eq!(embedding.provider_type, EmbeddingProviderType::FastEmbed);
            assert_eq!(embedding.model.name, "nomic-embed-text-v1.5");
            assert_eq!(embedding.model.dimensions, Some(768));
            assert_eq!(embedding.api.base_url, Some("local".to_string()));
        }
        Err(e) => {
            println!("✗ Regression test failed!");
            println!("{}", e);
            panic!("Regression test failed: The flatten issue may have returned! Error: {}", e);
        }
    }
}

/// Test that ProfileConfig.settings is no longer flattened
#[test]
fn test_profile_settings_not_flattened() {
    use crucible_config::ProfileConfig;

    let toml_config = r#"
name = "test"
environment = "test"

[settings]
custom_key = "custom_value"
"#;

    println!("=== Testing ProfileConfig with explicit settings table ===\n{}\n", toml_config);

    let result: Result<ProfileConfig, _> = toml::from_str(toml_config);
    match result {
        Ok(profile) => {
            println!("✓ ProfileConfig parsed successfully with settings table!");
            assert_eq!(profile.name, "test");
            // Settings should be in the settings HashMap, not flattened
            assert!(profile.settings.contains_key("custom_key"));
        }
        Err(e) => {
            println!("✗ Failed to parse ProfileConfig with settings table:");
            println!("{}", e);
            panic!("ProfileConfig settings parsing failed: {}", e);
        }
    }
}
