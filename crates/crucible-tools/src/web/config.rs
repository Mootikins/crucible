//! `WebTools` initialization and main interface

use super::cache::FetchCache;
use super::fetch::{create_client, fetch_and_convert, FetchError};
use super::search::{SearchError, SearchResult, SearxngProvider, WebSearchProvider};
use crucible_config::WebToolsConfig;
use reqwest::Client;
use std::path::PathBuf;
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

    /// I/O error (e.g., saving artifact)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Web tools container
///
/// Holds configuration, HTTP client, cache, and search provider.
/// Optionally has access to session context for saving artifacts.
pub struct WebTools {
    config: WebToolsConfig,
    client: Client,
    cache: Mutex<FetchCache>,
    search_provider: Option<Arc<dyn WebSearchProvider>>,
    /// Session directory for saving fetched content as artifacts
    session_dir: Option<PathBuf>,
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
            session_dir: None,
        }
    }

    /// Set the session directory for saving fetched content as artifacts
    ///
    /// When set, `web_fetch` will save content to `<session_dir>/artifacts/fetched/`
    #[must_use]
    pub fn with_session_dir(mut self, session_dir: PathBuf) -> Self {
        self.session_dir = Some(session_dir);
        self
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

        // Save to session artifacts if session_dir is set
        if let Some(ref session_dir) = self.session_dir {
            self.save_fetch_artifact(session_dir, url, &content)?;
        }

        // Cache the result
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(url.to_string(), content.clone());
        }

        Ok(content)
    }

    /// Save fetched content as a session artifact
    #[allow(clippy::unused_self)] // Method kept on self for future extensibility
    fn save_fetch_artifact(
        &self,
        session_dir: &std::path::Path,
        url: &str,
        content: &str,
    ) -> Result<(), WebToolsError> {
        use std::fs;

        // Create artifacts/fetched directory
        let artifacts_dir = session_dir.join("artifacts").join("fetched");
        fs::create_dir_all(&artifacts_dir)?;

        // Generate filename from URL (sanitize for filesystem)
        let filename = url_to_filename(url);
        let filepath = artifacts_dir.join(format!("{filename}.md"));

        // Write content
        fs::write(&filepath, content)?;

        Ok(())
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

/// Convert URL to a filesystem-safe filename
///
/// Extracts the hostname and path, replacing unsafe characters with underscores.
fn url_to_filename(url: &str) -> String {
    // Try to parse as URL, fall back to sanitizing the whole string
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("unknown");
        let path = parsed.path().trim_matches('/');
        if path.is_empty() {
            host.to_string()
        } else {
            format!("{host}_{}", path.replace('/', "_"))
        }
    } else {
        // Fallback: just sanitize the string
        url.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect()
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

    #[test]
    fn test_url_to_filename_simple() {
        assert_eq!(url_to_filename("https://example.com"), "example.com");
        assert_eq!(
            url_to_filename("https://example.com/path/to/page"),
            "example.com_path_to_page"
        );
        assert_eq!(
            url_to_filename("https://docs.rust-lang.org/book/"),
            "docs.rust-lang.org_book"
        );
    }

    #[test]
    fn test_url_to_filename_invalid_url() {
        // Invalid URLs get sanitized
        let result = url_to_filename("not a valid url");
        assert!(result.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
    }

    #[tokio::test]
    async fn test_fetch_saves_artifact_when_session_dir_set() {
        use tempfile::TempDir;

        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new().unwrap();

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("Test content"),
            )
            .mount(&mock_server)
            .await;

        let tools = WebTools::new(&enabled_config())
            .with_session_dir(temp_dir.path().to_path_buf());

        let _ = tools.fetch(&mock_server.uri(), "test", false).await.unwrap();

        // Check artifact was saved
        let artifacts_dir = temp_dir.path().join("artifacts").join("fetched");
        assert!(artifacts_dir.exists());

        // Should have created a file with the hostname
        let entries: Vec<_> = std::fs::read_dir(&artifacts_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);

        let content = std::fs::read_to_string(entries[0].path()).unwrap();
        assert_eq!(content, "Test content");
    }
}
