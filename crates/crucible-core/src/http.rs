//! HTTP request/response types shared between scripting runtimes.
//!
//! Provides a unified HTTP API for Lua and Rune scripts to make network requests.
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_core::http::{HttpRequest, HttpExecutor};
//!
//! let executor = HttpExecutor::new();
//! let request = HttpRequest::get("https://api.example.com/data")
//!     .header("Authorization", "Bearer token");
//!
//! let response = executor.execute(request).await?;
//! println!("Status: {}", response.status);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// HTTP request configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    /// Target URL
    pub url: String,
    /// HTTP method
    pub method: HttpMethod,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Request body (for POST, PUT, PATCH)
    pub body: Option<String>,
    /// Request timeout
    pub timeout: Duration,
}

impl HttpRequest {
    /// Create a GET request.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method: HttpMethod::Get,
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
        }
    }

    /// Create a POST request.
    pub fn post(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method: HttpMethod::Post,
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
        }
    }

    /// Create a PUT request.
    pub fn put(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method: HttpMethod::Put,
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
        }
    }

    /// Create a DELETE request.
    pub fn delete(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method: HttpMethod::Delete,
            headers: HashMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
        }
    }

    /// Add a header (builder pattern).
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the request body (builder pattern).
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Set the timeout (builder pattern).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// HTTP methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    /// Get the method as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// HTTP response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body
    pub body: String,
}

impl HttpResponse {
    /// Check if the response status indicates success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Parse the body as JSON.
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.body)
    }
}

/// HTTP error types.
#[derive(Debug, Clone)]
pub enum HttpError {
    /// Request failed to send
    Request(String),
    /// Failed to read response body
    Body(String),
    /// Request timed out
    Timeout,
    /// Invalid URL
    InvalidUrl(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(e) => write!(f, "HTTP request failed: {}", e),
            Self::Body(e) => write!(f, "Failed to read body: {}", e),
            Self::Timeout => write!(f, "Request timed out"),
            Self::InvalidUrl(e) => write!(f, "Invalid URL: {}", e),
        }
    }
}

impl std::error::Error for HttpError {}

/// HTTP executor using reqwest.
#[derive(Clone)]
pub struct HttpExecutor {
    client: reqwest::Client,
}

impl Default for HttpExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpExecutor {
    /// Create a new HTTP executor.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Execute an HTTP request.
    pub async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = match req.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        let mut builder = self
            .client
            .request(method, &req.url)
            .timeout(req.timeout);

        for (key, value) in &req.headers {
            builder = builder.header(key, value);
        }

        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let response = builder
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    HttpError::Timeout
                } else {
                    HttpError::Request(e.to_string())
                }
            })?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();

        let body = response
            .text()
            .await
            .map_err(|e| HttpError::Body(e.to_string()))?;

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_builder() {
        let req = HttpRequest::get("https://example.com")
            .header("Authorization", "Bearer token")
            .timeout(Duration::from_secs(60));

        assert_eq!(req.url, "https://example.com");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.headers.get("Authorization"), Some(&"Bearer token".to_string()));
        assert_eq!(req.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_http_request_post_with_body() {
        let req = HttpRequest::post("https://api.example.com")
            .header("Content-Type", "application/json")
            .body(r#"{"key": "value"}"#);

        assert_eq!(req.method, HttpMethod::Post);
        assert_eq!(req.body, Some(r#"{"key": "value"}"#.to_string()));
    }

    #[test]
    fn test_http_response_is_success() {
        let success = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: String::new(),
        };
        assert!(success.is_success());

        let redirect = HttpResponse {
            status: 301,
            headers: HashMap::new(),
            body: String::new(),
        };
        assert!(!redirect.is_success());

        let error = HttpResponse {
            status: 404,
            headers: HashMap::new(),
            body: String::new(),
        };
        assert!(!error.is_success());
    }

    #[test]
    fn test_http_response_json() {
        let response = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: r#"{"name": "test", "value": 42}"#.to_string(),
        };

        #[derive(Debug, Deserialize, PartialEq)]
        struct Data {
            name: String,
            value: i32,
        }

        let data: Data = response.json().unwrap();
        assert_eq!(data.name, "test");
        assert_eq!(data.value, 42);
    }

    #[test]
    fn test_http_method_as_str() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
    }

    #[test]
    fn test_http_error_display() {
        let req_err = HttpError::Request("connection refused".to_string());
        assert!(req_err.to_string().contains("connection refused"));

        let timeout_err = HttpError::Timeout;
        assert!(timeout_err.to_string().contains("timed out"));
    }
}
