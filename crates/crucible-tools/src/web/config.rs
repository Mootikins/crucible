//! WebTools initialization and main interface

use super::cache::FetchCache;
use super::fetch::{create_client, fetch_and_convert, FetchError};
use crucible_config::WebToolsConfig;
use reqwest::Client;
use std::sync::Mutex;
use thiserror::Error;

/// Errors from web tools operations
#[derive(Error, Debug)]
pub enum WebToolsError {
    #[error("Web tools are not enabled in configuration")]
    Disabled,

    #[error("Fetch error: {0}")]
    Fetch(#[from] FetchError),
}

/// Web tools container
///
/// Holds configuration, HTTP client, and cache. Provides fetch/search operations.
pub struct WebTools {
    config: WebToolsConfig,
    client: Client,
    cache: Mutex<FetchCache>,
}

impl WebTools {
    /// Create new WebTools from configuration
    pub fn new(config: &WebToolsConfig) -> Self {
        let cache = FetchCache::new(
            config.fetch.cache_ttl_secs,
            100, // Max 100 cached entries
        );

        Self {
            config: config.clone(),
            client: create_client(),
            cache: Mutex::new(cache),
        }
    }

    /// Check if web tools are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Fetch a URL and convert to markdown
    ///
    /// Returns cached content if available and not expired.
    ///
    /// # Arguments
    /// * `url` - URL to fetch
    /// * `_prompt` - Question about the content (used for summarization, not yet implemented)
    /// * `_summarize` - Whether to summarize with LLM (not yet implemented)
    ///
    /// # Errors
    /// Returns error if web tools disabled or fetch fails
    pub async fn fetch(
        &self,
        url: &str,
        _prompt: &str,
        _summarize: bool,
    ) -> Result<String, WebToolsError> {
        if !self.config.enabled {
            return Err(WebToolsError::Disabled);
        }

        // Check cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(url) {
                return Ok(cached.to_string());
            }
        }

        // Fetch and convert
        let content = fetch_and_convert(
            &self.client,
            url,
            &self.config.fetch.user_agent,
            self.config.fetch.timeout_secs,
            self.config.fetch.max_content_kb,
        )
        .await?;

        // TODO: If summarize=true and summarize_model is configured,
        // send content + prompt to LLM and return summary instead

        // TODO: Option to save markdown to session folder

        // Cache the result
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(url.to_string(), content.clone());
        }

        Ok(content)
    }
}

// Can't derive Clone due to Mutex, but we can implement it
impl Clone for WebTools {
    fn clone(&self) -> Self {
        Self::new(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn enabled_config() -> WebToolsConfig {
        WebToolsConfig {
            enabled: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_fetch_disabled_returns_error() {
        let config = WebToolsConfig::default(); // disabled by default
        let tools = WebTools::new(&config);

        let result = tools.fetch("https://example.com", "test", false).await;
        assert!(matches!(result, Err(WebToolsError::Disabled)));
    }

    #[tokio::test]
    async fn test_fetch_returns_markdown() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string("<h1>Test Page</h1>"),
            )
            .mount(&mock_server)
            .await;

        let tools = WebTools::new(&enabled_config());
        let result = tools
            .fetch(&mock_server.uri(), "describe", false)
            .await
            .unwrap();

        assert!(result.contains("Test Page"));
    }

    #[tokio::test]
    async fn test_fetch_uses_cache() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string("content"))
            .expect(1) // Should only be called once due to caching
            .mount(&mock_server)
            .await;

        let tools = WebTools::new(&enabled_config());

        // First fetch
        let _ = tools
            .fetch(&mock_server.uri(), "test", false)
            .await
            .unwrap();

        // Second fetch should use cache
        let _ = tools
            .fetch(&mock_server.uri(), "test", false)
            .await
            .unwrap();

        // Mock expectation will verify only 1 request was made
    }
}
