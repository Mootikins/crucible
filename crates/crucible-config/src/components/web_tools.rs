//! Web tools configuration for fetch and search operations

use serde::{Deserialize, Serialize};

/// Configuration for web tools (fetch and search)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebToolsConfig {
    /// Whether web tools are enabled (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Fetch configuration
    #[serde(default)]
    pub fetch: FetchConfig,

    /// Search configuration
    #[serde(default)]
    pub search: SearchConfig,
}

/// Configuration for web_fetch tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchConfig {
    /// Cache TTL in seconds (default: 900 = 15 minutes)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,

    /// Maximum content size in KB (default: 100)
    #[serde(default = "default_max_content")]
    pub max_content_kb: u32,

    /// User agent string for requests
    #[serde(default = "default_user_agent")]
    pub user_agent: String,

    /// Request timeout in seconds (default: 30)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Optional LLM model for summarization (e.g., "claude-3-haiku")
    #[serde(default)]
    pub summarize_model: Option<String>,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            cache_ttl_secs: default_cache_ttl(),
            max_content_kb: default_max_content(),
            user_agent: default_user_agent(),
            timeout_secs: default_timeout(),
            summarize_model: None,
        }
    }
}

/// Configuration for web_search tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Search provider (currently only "searxng")
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Default result limit (default: 10)
    #[serde(default = "default_limit")]
    pub limit_default: u32,

    /// SearXNG-specific configuration
    #[serde(default)]
    pub searxng: Option<SearxngConfig>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            limit_default: default_limit(),
            searxng: Some(SearxngConfig::default()),
        }
    }
}

/// SearXNG provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearxngConfig {
    /// SearXNG instance URL (default: public instance)
    #[serde(default = "default_searxng_url")]
    pub url: String,

    /// Optional HTTP basic auth username
    #[serde(default)]
    pub auth_user: Option<String>,

    /// Optional HTTP basic auth password
    #[serde(default)]
    pub auth_password: Option<String>,
}

impl Default for SearxngConfig {
    fn default() -> Self {
        Self {
            url: default_searxng_url(),
            auth_user: None,
            auth_password: None,
        }
    }
}

// Default value functions
fn default_cache_ttl() -> u64 {
    900
}
fn default_max_content() -> u32 {
    100
}
fn default_user_agent() -> String {
    "Crucible/1.0".to_string()
}
fn default_timeout() -> u64 {
    30
}
fn default_provider() -> String {
    "searxng".to_string()
}
fn default_limit() -> u32 {
    10
}
fn default_searxng_url() -> String {
    "https://searx.be".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_disabled() {
        let config = WebToolsConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_fetch_defaults() {
        let config = FetchConfig::default();
        assert_eq!(config.cache_ttl_secs, 900);
        assert_eq!(config.max_content_kb, 100);
        assert_eq!(config.user_agent, "Crucible/1.0");
        assert_eq!(config.timeout_secs, 30);
        assert!(config.summarize_model.is_none());
    }

    #[test]
    fn test_search_defaults() {
        let config = SearchConfig::default();
        assert_eq!(config.provider, "searxng");
        assert_eq!(config.limit_default, 10);
        assert!(config.searxng.is_some());
    }

    #[test]
    fn test_searxng_default_url() {
        let config = SearxngConfig::default();
        assert_eq!(config.url, "https://searx.be");
        assert!(config.auth_user.is_none());
    }

    #[test]
    fn test_parse_toml_enabled() {
        let toml_content = r#"
enabled = true

[fetch]
cache_ttl_secs = 600
max_content_kb = 200
summarize_model = "claude-3-haiku"

[search]
provider = "searxng"
limit_default = 20

[search.searxng]
url = "http://localhost:8080"
auth_user = "admin"
auth_password = "secret"
"#;

        let config: WebToolsConfig = toml::from_str(toml_content).unwrap();
        assert!(config.enabled);
        assert_eq!(config.fetch.cache_ttl_secs, 600);
        assert_eq!(config.fetch.max_content_kb, 200);
        assert_eq!(
            config.fetch.summarize_model,
            Some("claude-3-haiku".to_string())
        );
        assert_eq!(config.search.limit_default, 20);

        let searxng = config.search.searxng.unwrap();
        assert_eq!(searxng.url, "http://localhost:8080");
        assert_eq!(searxng.auth_user, Some("admin".to_string()));
    }

    #[test]
    fn test_parse_toml_minimal() {
        let toml_content = r#"
enabled = true
"#;

        let config: WebToolsConfig = toml::from_str(toml_content).unwrap();
        assert!(config.enabled);
        assert_eq!(config.fetch.cache_ttl_secs, 900);
        assert_eq!(config.search.provider, "searxng");
    }
}
