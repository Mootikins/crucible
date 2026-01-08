//! Web page fetching and HTML to markdown conversion

use reqwest::Client;
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur during fetch operations
#[derive(Error, Debug)]
pub enum FetchError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    /// Request timed out
    #[error("Request timeout after {0} seconds")]
    Timeout(u64),

    /// Content exceeded size limit
    #[error("Content too large: {size_kb} KB (max: {max_kb} KB)")]
    ContentTooLarge {
        /// Actual content size in KB
        size_kb: u32,
        /// Maximum allowed size in KB
        max_kb: u32,
    },

    /// Invalid URL provided
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

/// Fetch a URL and convert HTML to markdown
pub async fn fetch_and_convert(
    client: &Client,
    url: &str,
    user_agent: &str,
    timeout_secs: u64,
    max_content_kb: u32,
) -> Result<String, FetchError> {
    let response = client
        .get(url)
        .header("User-Agent", user_agent)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .await?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = response.text().await?;

    // Check size
    let size_kb = (body.len() / 1024) as u32;
    if size_kb > max_content_kb {
        // Truncate instead of error
        let max_bytes = (max_content_kb as usize) * 1024;
        let truncated = body.chars().take(max_bytes).collect::<String>();
        let markdown = html_to_markdown(&truncated, &content_type);
        return Ok(format!(
            "{}\n\n[Content truncated - exceeded {} KB limit]",
            markdown, max_content_kb
        ));
    }

    let markdown = html_to_markdown(&body, &content_type);
    Ok(markdown)
}

/// Convert HTML to markdown, or pass through if not HTML
fn html_to_markdown(content: &str, content_type: &str) -> String {
    // If it's already markdown or plain text, return as-is
    if content_type.contains("text/plain") || content_type.contains("text/markdown") {
        return content.to_string();
    }

    // Check if content looks like HTML
    let trimmed = content.trim();
    if !trimmed.starts_with('<')
        && !trimmed.contains("<!DOCTYPE")
        && !trimmed.contains("<html")
    {
        // Doesn't look like HTML, return as-is
        return content.to_string();
    }

    // Convert HTML to markdown using htmd
    // TODO: Add hook point for readability preprocessing here
    htmd::convert(content).unwrap_or_else(|_| content.to_string())
}

/// Create an HTTP client with default settings
pub fn create_client() -> Client {
    Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("Failed to create HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_fetch_html_converts_to_markdown() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string("<h1>Hello World</h1><p>This is a test.</p>"),
            )
            .mount(&mock_server)
            .await;

        let client = create_client();
        let result = fetch_and_convert(&client, &mock_server.uri(), "Test/1.0", 30, 100)
            .await
            .unwrap();

        assert!(result.contains("Hello World"));
        assert!(result.contains("This is a test"));
    }

    #[tokio::test]
    async fn test_fetch_plain_text_passthrough() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("Plain text content"),
            )
            .mount(&mock_server)
            .await;

        let client = create_client();
        let result = fetch_and_convert(&client, &mock_server.uri(), "Test/1.0", 30, 100)
            .await
            .unwrap();

        assert_eq!(result, "Plain text content");
    }

    #[tokio::test]
    async fn test_fetch_truncates_large_content() {
        let mock_server = MockServer::start().await;

        // Create content larger than 1KB
        let large_content = "x".repeat(2048);

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string(&large_content),
            )
            .mount(&mock_server)
            .await;

        let client = create_client();
        let result = fetch_and_convert(
            &client,
            &mock_server.uri(),
            "Test/1.0",
            30,
            1, // 1 KB max
        )
        .await
        .unwrap();

        assert!(result.contains("[Content truncated"));
        assert!(result.len() < large_content.len());
    }

    #[tokio::test]
    async fn test_fetch_sends_user_agent() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/"))
            .and(wiremock::matchers::header("User-Agent", "CustomAgent/2.0"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = create_client();
        let _ = fetch_and_convert(&client, &mock_server.uri(), "CustomAgent/2.0", 30, 100).await;

        // Expectation will fail if User-Agent wasn't sent correctly
    }

    #[test]
    fn test_html_to_markdown_basic() {
        let html = "<h1>Title</h1><p>Paragraph</p>";
        let md = html_to_markdown(html, "text/html");

        assert!(md.contains("Title"));
        assert!(md.contains("Paragraph"));
    }

    #[test]
    fn test_html_to_markdown_non_html_passthrough() {
        let content = "Just plain text without HTML";
        let md = html_to_markdown(content, "text/html");

        assert_eq!(md, content);
    }
}
