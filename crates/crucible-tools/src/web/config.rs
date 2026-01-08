//! `WebTools` initialization and main interface

use super::cache::FetchCache;
use super::fetch::{create_client, fetch_and_convert, FetchError};
use super::search::{SearchError, SearchResult, SearxngProvider, WebSearchProvider};
use crucible_config::WebToolsConfig;
use reqwest::Client;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Errors from web tools operations
#[derive(Error, Debug)]
pub enum WebToolsError {
    /// Web tools are disabled in configuration
    #[error("Web tools are not enabled in configuration")]
    Disabled,

    /// Error during fetch operation
    #[error("Fetch error: {0}")]
    Fetch(#[from] FetchError),

    /// Error during search operation
    #[error("Search error: {0}")]
    Search(#[from] SearchError),

    /// Search provider not supported or not configured
    #[error("Search provider '{0}' not supported")]
    UnsupportedProvider(String),
}

/// Web tools container
///
/// Holds configuration, HTTP client, cache, and search provider.
pub struct WebTools {
    config: WebToolsConfig,
    client: Client,
    cache: Mutex<FetchCache>,
    search_provider: Option<Arc<dyn WebSearchProvider>>,
}

impl WebTools {
    /// Create new `WebTools` from configuration
    #[must_use]
    pub fn new(config: &WebToolsConfig) -> Self {
        let cache = FetchCache::new(
            config.fetch.cache_ttl_secs,
            100, // Max 100 cached entries
        );

        let client = create_client();

        // Initialize search provider if configured
        let search_provider: Option<Arc<dyn WebSearchProvider>> =
            if config.search.provider == "searxng" {
                config.search.searxng.as_ref().map(|searxng_config| {
                    Arc::new(SearxngProvider::new(client.clone(), searxng_config.clone()))
                        as Arc<dyn WebSearchProvider>
                })
            } else {
                None
            };

        Self {
            config: config.clone(),
            client,
            cache: Mutex::new(cache),
            search_provider,
        }
    }

    /// Check if web tools are enabled
    #[must_use]
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
    /// Returns error if web tools disabled or fetch fails.
    ///
    /// # Panics
    /// Panics if the internal cache mutex is poisoned.
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

    /// Search the web using configured provider
    ///
    /// # Arguments
    /// * `query` - Search query
    /// * `limit` - Maximum number of results (uses config default if None)
    ///
    /// # Errors
    /// Returns error if web tools disabled, search provider not configured, or search fails.
    pub async fn search(
        &self,
        query: &str,
        limit: Option<u32>,
    ) -> Result<Vec<SearchResult>, WebToolsError> {
        if !self.config.enabled {
            return Err(WebToolsError::Disabled);
        }

        let provider = self
            .search_provider
            .as_ref()
            .ok_or_else(|| WebToolsError::UnsupportedProvider(self.config.search.provider.clone()))?;

        let limit = limit.unwrap_or(self.config.search.limit_default);
        let results = provider.search(query, limit).await?;

        Ok(results)
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
    use crucible_config::{FetchConfig, SearchConfig, SearxngConfig};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn enabled_config() -> WebToolsConfig {
        WebToolsConfig {
            enabled: true,
            ..Default::default()
        }
    }

    fn enabled_config_with_searxng(url: String) -> WebToolsConfig {
        WebToolsConfig {
            enabled: true,
            fetch: FetchConfig::default(),
            search: SearchConfig {
                provider: "searxng".to_string(),
                limit_default: 10,
                searxng: Some(SearxngConfig {
                    url,
                    auth_user: None,
                    auth_password: None,
                }),
            },
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

    #[tokio::test]
    async fn test_search_disabled_returns_error() {
        let config = WebToolsConfig::default();
        let tools = WebTools::new(&config);

        let result = tools.search("test", None).await;
        assert!(matches!(result, Err(WebToolsError::Disabled)));
    }

    #[tokio::test]
    async fn test_search_returns_results() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    r#"{"results": [{"title": "Test", "url": "https://example.com", "content": "desc"}]}"#,
                ),
            )
            .mount(&mock_server)
            .await;

        let tools = WebTools::new(&enabled_config_with_searxng(mock_server.uri()));
        let results = tools.search("test", Some(10)).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Test");
    }

    #[tokio::test]
    async fn test_search_uses_default_limit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"results": []}"#))
            .mount(&mock_server)
            .await;

        let config = enabled_config_with_searxng(mock_server.uri());
        let tools = WebTools::new(&config);

        let results = tools.search("test", None).await.unwrap();
        assert!(results.is_empty());
    }
}
