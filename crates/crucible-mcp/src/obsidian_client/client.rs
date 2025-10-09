// crates/crucible-mcp/src/obsidian_client/client.rs

//! HTTP client for the Obsidian plugin API

use super::{
    config::{ClientConfig, RetryConfig},
    error::{ObsidianError, Result},
    types::*,
};
use reqwest::{Client, Response, StatusCode};
use std::collections::HashMap;
use tokio::time::sleep;
use tracing::{debug, warn};

/// HTTP client for interacting with the Obsidian plugin API
#[derive(Clone)]
pub struct ObsidianClient {
    client: Client,
    base_url: String,
    retry_config: RetryConfig,
}

impl ObsidianClient {
    /// Create a new Obsidian client with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(ClientConfig::default())
    }

    /// Create a new Obsidian client with custom port
    pub fn with_port(port: u16) -> Result<Self> {
        let config = ClientConfig::builder().port(port).build();
        Self::with_config(config)
    }

    /// Create a new Obsidian client with custom configuration
    pub fn with_config(config: ClientConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .pool_max_idle_per_host(config.max_idle_per_host)
            .build()
            .map_err(ObsidianError::RequestFailed)?;

        Ok(Self {
            client,
            base_url: config.base_url(),
            retry_config: config.retry,
        })
    }

    // ===== File Operations =====

    /// List all markdown files in the vault
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let files = client.list_files().await?;
    /// for file in files {
    ///     println!("{}: {} bytes", file.path, file.size);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_files(&self) -> Result<Vec<FileInfo>> {
        debug!("Listing all files");

        self.retry_request(|| async {
            let url = format!("{}/api/files", self.base_url);
            let response = self.client.get(&url).send().await?;
            let list: ListFilesResponse = self.handle_response(response).await?;
            Ok(list.files)
        })
        .await
    }

    /// Get file content by path
    ///
    /// # Arguments
    /// * `path` - The vault-relative path to the file (e.g., "folder/note.md")
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let content = client.get_file("daily/2024-01-01.md").await?;
    /// println!("{}", content);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_file(&self, path: &str) -> Result<String> {
        debug!("Getting file: {}", path);

        self.retry_request(|| async {
            let encoded_path = urlencoding::encode(path);
            let url = format!("{}/api/file/{}", self.base_url, encoded_path);
            let response = self.client.get(&url).send().await?;
            let file_content: FileContentResponse = self.handle_response(response).await?;
            Ok(file_content.content)
        })
        .await
    }

    /// Get file metadata including properties, tags, links, and stats
    ///
    /// # Arguments
    /// * `path` - The vault-relative path to the file (e.g., "folder/note.md")
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let metadata = client.get_metadata("daily/2024-01-01.md").await?;
    /// println!("Tags: {:?}", metadata.tags);
    /// println!("Links: {:?}", metadata.links);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_metadata(&self, path: &str) -> Result<FileMetadata> {
        debug!("Getting metadata for: {}", path);

        self.retry_request(|| async {
            let encoded_path = urlencoding::encode(path);
            let url = format!("{}/api/file/{}/metadata", self.base_url, encoded_path);
            let response = self.client.get(&url).send().await?;
            self.handle_response(response).await
        })
        .await
    }

    // ===== Search Operations =====

    /// Search files by tags
    ///
    /// # Arguments
    /// * `tags` - List of tags to search for
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let files = client.search_by_tags(&["project".to_string(), "active".to_string()]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<FileInfo>> {
        debug!("Searching by tags: {:?}", tags);

        self.retry_request(|| async {
            let url = format!("{}/api/search/tags", self.base_url);
            let response = self
                .client
                .get(&url)
                .query(&[("tags", tags.join(","))])
                .send()
                .await?;
            let search_result: SearchResponse = self.handle_response(response).await?;
            Ok(search_result.files)
        })
        .await
    }

    /// Search files in a folder
    ///
    /// # Arguments
    /// * `path` - The folder path to search in
    /// * `recursive` - Whether to search recursively in subfolders
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let files = client.search_by_folder("projects", true).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn search_by_folder(&self, path: &str, recursive: bool) -> Result<Vec<FileInfo>> {
        debug!("Searching in folder: {} (recursive: {})", path, recursive);

        self.retry_request(|| async {
            let url = format!("{}/api/search/folder", self.base_url);
            let response = self
                .client
                .get(&url)
                .query(&[("path", path), ("recursive", &recursive.to_string())])
                .send()
                .await?;
            let search_result: SearchResponse = self.handle_response(response).await?;
            Ok(search_result.files)
        })
        .await
    }

    /// Search files by frontmatter properties
    ///
    /// # Arguments
    /// * `properties` - Map of property names to values to search for
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # use std::collections::HashMap;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let mut props = HashMap::new();
    /// props.insert("status".to_string(), "active".to_string());
    /// let files = client.search_by_properties(&props).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn search_by_properties(
        &self,
        properties: &HashMap<String, String>,
    ) -> Result<Vec<FileInfo>> {
        debug!("Searching by properties: {:?}", properties);

        self.retry_request(|| async {
            let url = format!("{}/api/search/properties", self.base_url);

            // Convert HashMap to query parameters
            let query_params: Vec<(String, String)> = properties
                .iter()
                .map(|(k, v)| (format!("properties[{}]", k), v.clone()))
                .collect();

            let response = self.client.get(&url).query(&query_params).send().await?;
            let search_result: SearchResponse = self.handle_response(response).await?;
            Ok(search_result.files)
        })
        .await
    }

    /// Full-text search in file contents
    ///
    /// # Arguments
    /// * `query` - The search query string
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let files = client.search_by_content("machine learning").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn search_by_content(&self, query: &str) -> Result<Vec<FileInfo>> {
        debug!("Searching by content: {}", query);

        self.retry_request(|| async {
            let url = format!("{}/api/search/content", self.base_url);
            let response = self
                .client
                .get(&url)
                .query(&[("query", query)])
                .send()
                .await?;
            let search_result: SearchResponse = self.handle_response(response).await?;
            Ok(search_result.files)
        })
        .await
    }

    // ===== Property Operations =====

    /// Update frontmatter properties for a file
    ///
    /// # Arguments
    /// * `path` - The vault-relative path to the file
    /// * `properties` - Map of property names to values
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # use std::collections::HashMap;
    /// # use serde_json::json;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let mut props = HashMap::new();
    /// props.insert("status".to_string(), json!("completed"));
    /// props.insert("priority".to_string(), json!(5));
    /// let response = client.update_properties("tasks/task1.md", &props).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_properties(
        &self,
        path: &str,
        properties: &HashMap<String, serde_json::Value>,
    ) -> Result<UpdatePropertiesResponse> {
        debug!("Updating properties for: {}", path);

        self.retry_request(|| async {
            let encoded_path = urlencoding::encode(path);
            let url = format!("{}/api/file/{}/properties", self.base_url, encoded_path);

            let request_body = UpdatePropertiesRequest {
                properties: properties.clone(),
            };

            let response = self.client.put(&url).json(&request_body).send().await?;
            self.handle_response(response).await
        })
        .await
    }

    // ===== Settings Operations =====

    /// Get embedding provider configuration
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let settings = client.get_embedding_settings().await?;
    /// println!("Provider: {}, Model: {}", settings.provider, settings.model);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_embedding_settings(&self) -> Result<EmbeddingSettings> {
        debug!("Getting embedding settings");

        self.retry_request(|| async {
            let url = format!("{}/api/settings/embeddings", self.base_url);
            let response = self.client.get(&url).send().await?;
            self.handle_response(response).await
        })
        .await
    }

    /// Update embedding provider configuration
    ///
    /// # Arguments
    /// * `settings` - The embedding settings to update
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::{ObsidianClient, EmbeddingSettings};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let settings = EmbeddingSettings {
    ///     provider: "ollama".to_string(),
    ///     api_url: "http://localhost:11434".to_string(),
    ///     api_key: None,
    ///     model: "nomic-embed-text".to_string(),
    /// };
    /// let response = client.update_embedding_settings(&settings).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_embedding_settings(
        &self,
        settings: &EmbeddingSettings,
    ) -> Result<UpdateSettingsResponse> {
        debug!("Updating embedding settings");

        self.retry_request(|| async {
            let url = format!("{}/api/settings/embeddings", self.base_url);
            let response = self.client.put(&url).json(settings).send().await?;
            self.handle_response(response).await
        })
        .await
    }

    /// List available models from the embedding provider
    ///
    /// # Example
    /// ```no_run
    /// # use crucible_mcp::obsidian_client::ObsidianClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = ObsidianClient::new()?;
    /// let models = client.list_embedding_models().await?;
    /// for model in models {
    ///     println!("Available model: {}", model);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_embedding_models(&self) -> Result<Vec<String>> {
        debug!("Listing embedding models");

        self.retry_request(|| async {
            let url = format!("{}/api/settings/embeddings/models", self.base_url);
            let response = self.client.get(&url).send().await?;
            let models: EmbeddingModelsResponse = self.handle_response(response).await?;
            Ok(models.models)
        })
        .await
    }

    // ===== Helper Methods =====

    /// Handle HTTP response and deserialize JSON or return error
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T> {
        let status = response.status();

        if status.is_success() {
            response.json::<T>().await.map_err(|e| {
                warn!("Failed to parse response JSON: {}", e);
                ObsidianError::InvalidResponse(format!("Invalid JSON response: {}", e))
            })
        } else {
            let status_code = status.as_u16();

            // Try to parse error response
            let error_message = if let Ok(error_resp) = response.json::<ErrorResponse>().await {
                error_resp.message.unwrap_or(error_resp.error)
            } else {
                format!("HTTP {}", status_code)
            };

            // Handle specific status codes
            match status {
                StatusCode::NOT_FOUND => Err(ObsidianError::FileNotFound(error_message)),
                StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => {
                    Err(ObsidianError::Timeout)
                }
                _ => Err(ObsidianError::HttpError {
                    status: status_code,
                    message: error_message,
                }),
            }
        }
    }

    /// Retry a request with exponential backoff
    async fn retry_request<F, Fut, T>(&self, request_fn: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut attempts = 0;
        let mut delay = self.retry_config.initial_backoff;

        loop {
            match request_fn().await {
                Ok(result) => {
                    if attempts > 0 {
                        debug!("Request succeeded after {} retries", attempts);
                    }
                    return Ok(result);
                }
                Err(error) => {
                    attempts += 1;

                    // Check if we should retry
                    if !self.is_retriable(&error) || attempts > self.retry_config.max_retries {
                        if attempts > self.retry_config.max_retries {
                            warn!(
                                "Request failed after {} attempts: {}",
                                self.retry_config.max_retries, error
                            );
                            return Err(ObsidianError::TooManyRetries);
                        }
                        return Err(error);
                    }

                    // Log retry attempt
                    warn!(
                        "Request failed (attempt {}/{}), retrying in {:?}: {}",
                        attempts, self.retry_config.max_retries, delay, error
                    );

                    // Wait before retrying
                    sleep(delay).await;

                    // Exponential backoff with max cap
                    delay = std::cmp::min(delay * 2, self.retry_config.max_backoff);
                }
            }
        }
    }

    /// Check if an error is retriable
    fn is_retriable(&self, error: &ObsidianError) -> bool {
        match error {
            // Network errors are retriable
            ObsidianError::RequestFailed(e) => e.is_timeout() || e.is_connect() || e.is_request(),

            // Timeouts are retriable
            ObsidianError::Timeout => true,

            // HTTP errors
            ObsidianError::HttpError { status, .. } => {
                // Retry on 5xx server errors and 429 rate limiting
                *status >= 500 || *status == 429
            }

            // Don't retry these
            ObsidianError::FileNotFound(_)
            | ObsidianError::JsonError(_)
            | ObsidianError::InvalidResponse(_)
            | ObsidianError::InvalidConfig(_)
            | ObsidianError::TooManyRetries
            | ObsidianError::Other(_) => false,
        }
    }
}

impl Default for ObsidianClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default Obsidian client")
    }
}
