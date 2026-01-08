//! Web search provider trait and implementations

use async_trait::async_trait;
use crucible_config::SearxngConfig;
use reqwest::Client;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Search result from web search
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    /// Page title
    pub title: String,
    /// Page URL
    pub url: String,
    /// Optional snippet/description
    pub snippet: Option<String>,
}

/// Errors that can occur during search operations
#[derive(Error, Debug)]
pub enum SearchError {
    /// HTTP request to search provider failed
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// Failed to parse search results from provider
    #[error("Failed to parse search results: {0}")]
    Parse(String),

    /// Search provider is not configured
    #[error("Search provider not configured")]
    NotConfigured,
}

/// Trait for web search providers
#[async_trait]
pub trait WebSearchProvider: Send + Sync {
    /// Search the web and return results
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>, SearchError>;
}

/// `SearXNG` search provider
pub struct SearxngProvider {
    client: Client,
    config: SearxngConfig,
}

impl SearxngProvider {
    /// Create a new `SearXNG` provider
    #[must_use]
    pub fn new(client: Client, config: SearxngConfig) -> Self {
        Self { client, config }
    }
}

#[async_trait]
impl WebSearchProvider for SearxngProvider {
    async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>, SearchError> {
        let url = format!(
            "{}/search?q={}&format=json&pageno=1",
            self.config.url.trim_end_matches('/'),
            urlencoding::encode(query)
        );

        let mut request = self.client.get(&url);

        // Add basic auth if configured
        if let (Some(user), Some(pass)) = (&self.config.auth_user, &self.config.auth_password) {
            request = request.basic_auth(user, Some(pass));
        }

        let response = request.send().await?;
        let body: SearxngResponse = response.json().await?;

        let results: Vec<SearchResult> = body
            .results
            .into_iter()
            .take(limit as usize)
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.content,
            })
            .collect();

        Ok(results)
    }
}

/// `SearXNG` JSON response structure
#[derive(Debug, Deserialize)]
struct SearxngResponse {
    results: Vec<SearxngResult>,
}

#[derive(Debug, Deserialize)]
struct SearxngResult {
    title: String,
    url: String,
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(url: String) -> SearxngConfig {
        SearxngConfig {
            url,
            auth_user: None,
            auth_password: None,
        }
    }

    #[tokio::test]
    async fn test_searxng_search_returns_results() {
        let mock_server = MockServer::start().await;

        let response_json = r#"{
            "results": [
                {"title": "Result 1", "url": "https://example.com/1", "content": "Description 1"},
                {"title": "Result 2", "url": "https://example.com/2", "content": "Description 2"}
            ]
        }"#;

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "test query"))
            .and(query_param("format", "json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let provider = SearxngProvider::new(client, test_config(mock_server.uri()));

        let results = provider.search("test query", 10).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Result 1");
        assert_eq!(results[0].url, "https://example.com/1");
        assert_eq!(results[0].snippet, Some("Description 1".to_string()));
    }

    #[tokio::test]
    async fn test_searxng_search_respects_limit() {
        let mock_server = MockServer::start().await;

        let response_json = r#"{
            "results": [
                {"title": "Result 1", "url": "https://example.com/1", "content": null},
                {"title": "Result 2", "url": "https://example.com/2", "content": null},
                {"title": "Result 3", "url": "https://example.com/3", "content": null}
            ]
        }"#;

        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_json))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let provider = SearxngProvider::new(client, test_config(mock_server.uri()));

        let results = provider.search("query", 2).await.unwrap();

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_searxng_with_auth() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(wiremock::matchers::header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"results": []}"#))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let config = SearxngConfig {
            url: mock_server.uri(),
            auth_user: Some("user".to_string()),
            auth_password: Some("pass".to_string()),
        };
        let provider = SearxngProvider::new(client, config);

        let _ = provider.search("query", 10).await;

        // Mock expectation verifies Authorization header was sent
    }

    #[test]
    fn test_search_result_serialization() {
        let result = SearchResult {
            title: "Test".to_string(),
            url: "https://example.com".to_string(),
            snippet: Some("A snippet".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("https://example.com"));
    }
}
