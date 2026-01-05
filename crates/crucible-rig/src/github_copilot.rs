//! GitHub Copilot provider with OAuth device flow authentication.
//!
//! This module provides integration with GitHub Copilot's API using the same
//! OAuth client ID as VS Code. It handles:
//!
//! - OAuth device flow authentication
//! - Automatic Copilot API token refresh (30-minute TTL)
//! - OpenAI-compatible chat completions
//! - Model listing
//!
//! # User Story
//!
//! See `docs/Guides/GitHub Copilot Setup.md` for the complete setup guide.
//!
//! **As a** Crucible user with a GitHub Copilot subscription,
//! **I want to** use my existing Copilot access for chat completions,
//! **So that** I can leverage models like GPT-4o without additional API costs.
//!
//! # Authentication Flow
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
//! │   Crucible  │────▶│   GitHub    │────▶│  GitHub Copilot  │
//! │     CLI     │     │    OAuth    │     │       API        │
//! └─────────────┘     └─────────────┘     └──────────────────┘
//!        │                                        │
//!        │ 1. cru auth copilot                    │
//!        │ 2. User visits github.com/login/device │
//!        │ 3. OAuth token (gho_xxx) stored        │
//!        │ 4. API token exchanged (30min TTL)     │
//!        │ 5. Chat requests auto-refresh token    │
//! ```
//!
//! # CLI Usage
//!
//! ```bash
//! # Authenticate (one-time)
//! cru auth copilot
//!
//! # List available models
//! cru models --provider copilot
//!
//! # Use in chat
//! cru chat --provider copilot
//! ```
//!
//! # Programmatic Example
//!
//! ```rust,ignore
//! use crucible_rig::github_copilot::{CopilotAuth, CopilotClient};
//!
//! // Start OAuth device flow
//! let auth = CopilotAuth::new();
//! let device_code = auth.start_device_flow().await?;
//! println!("Visit {} and enter code: {}", device_code.verification_uri, device_code.user_code);
//!
//! // Poll for completion (user authorizes in browser)
//! let oauth_token = auth.poll_for_token(&device_code).await?;
//!
//! // Create client with token
//! let client = CopilotClient::new(oauth_token.access_token);
//!
//! // List available models
//! let models = client.list_models().await?;
//! for model in &models {
//!     println!("{}", model.id);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

/// VS Code's OAuth client ID for GitHub Copilot
pub const COPILOT_CLIENT_ID: &str = "01ab8ac9400c4e429b23";

/// GitHub OAuth device code endpoint
pub const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// GitHub OAuth token endpoint
pub const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// GitHub Copilot internal token endpoint
pub const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// GitHub Copilot API base URL
pub const COPILOT_API_BASE: &str = "https://api.githubcopilot.com";

/// Errors from GitHub Copilot operations
#[derive(Debug, Error)]
pub enum CopilotError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// OAuth flow failed
    #[error("OAuth flow failed: {0}")]
    OAuth(String),

    /// Token exchange failed
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),

    /// API error
    #[error("Copilot API error: {status} - {message}")]
    Api {
        /// HTTP status code
        status: u16,
        /// Error message from API
        message: String,
    },

    /// Authorization pending (user hasn't completed device flow yet)
    #[error("Authorization pending - user must complete device flow")]
    AuthorizationPending,

    /// Rate limited - caller should increase polling interval
    #[error("Rate limited - increase polling interval by 5 seconds")]
    SlowDown,

    /// Device flow expired
    #[error("Device code expired - please restart authentication")]
    DeviceCodeExpired,

    /// Access denied
    #[error("Access denied - user cancelled or doesn't have Copilot access")]
    AccessDenied,

    /// Serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for Copilot operations
pub type CopilotResult<T> = Result<T, CopilotError>;

/// Device code response from GitHub OAuth
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    /// The device code for token polling
    pub device_code: String,
    /// The code the user must enter
    pub user_code: String,
    /// URL where user should enter the code
    pub verification_uri: String,
    /// Seconds until device code expires
    pub expires_in: u64,
    /// Polling interval in seconds
    pub interval: u64,
}

/// OAuth token response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    /// The access token
    pub access_token: String,
    /// Token type (usually "bearer")
    pub token_type: String,
    /// OAuth scopes granted
    pub scope: String,
}

/// Copilot API token response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotTokenResponse {
    /// The Copilot API token
    pub token: String,
    /// Token expiration timestamp (Unix epoch)
    pub expires_at: i64,
    /// Endpoints configuration
    #[serde(default)]
    pub endpoints: CopilotEndpoints,
}

/// Copilot API endpoints from token response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CopilotEndpoints {
    /// API base URL
    #[serde(default)]
    pub api: String,
    /// Proxy URL (if any)
    #[serde(default)]
    pub proxy: String,
}

/// Available model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotModel {
    /// Model identifier
    pub id: String,
    /// Model name
    #[serde(default)]
    pub name: String,
    /// Model version
    #[serde(default)]
    pub version: String,
    /// Whether model supports chat
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

/// Model capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Supports chat completions
    #[serde(default)]
    pub chat: bool,
    /// Supports embeddings
    #[serde(default)]
    pub embeddings: bool,
}

/// Models list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    /// List of available models
    pub data: Vec<CopilotModel>,
}

/// Cached Copilot API token with expiration
#[derive(Debug)]
struct CachedToken {
    token: String,
    expires_at: Instant,
    api_base: String,
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        // Refresh 5 minutes before expiration for safety
        Instant::now() > self.expires_at - Duration::from_secs(300)
    }
}

/// GitHub Copilot OAuth authentication handler
pub struct CopilotAuth {
    client: reqwest::Client,
}

impl Default for CopilotAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl CopilotAuth {
    /// Create a new authentication handler
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Start the OAuth device flow
    ///
    /// Returns device code information. The user must visit the verification URI
    /// and enter the user code to authorize.
    pub async fn start_device_flow(&self) -> CopilotResult<DeviceCodeResponse> {
        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", COPILOT_CLIENT_ID),
                ("scope", "user:email"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(CopilotError::OAuth(format!(
                "Device flow failed: {} - {}",
                status, text
            )));
        }

        let device_code: DeviceCodeResponse = response.json().await?;
        Ok(device_code)
    }

    /// Poll for OAuth token after user completes device flow
    ///
    /// This should be called repeatedly with the interval from [`DeviceCodeResponse`]
    /// until the user completes authorization or the device code expires.
    pub async fn poll_for_token(
        &self,
        device_code: &DeviceCodeResponse,
    ) -> CopilotResult<OAuthTokenResponse> {
        let response = self
            .client
            .post(TOKEN_URL)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", COPILOT_CLIENT_ID),
                ("device_code", &device_code.device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        // Check for error responses
        if let Ok(error_response) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(error) = error_response.get("error").and_then(|e| e.as_str()) {
                return match error {
                    "authorization_pending" => Err(CopilotError::AuthorizationPending),
                    "slow_down" => Err(CopilotError::SlowDown),
                    "expired_token" => Err(CopilotError::DeviceCodeExpired),
                    "access_denied" => Err(CopilotError::AccessDenied),
                    _ => Err(CopilotError::OAuth(format!(
                        "OAuth error: {} - {}",
                        error,
                        error_response
                            .get("error_description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("unknown")
                    ))),
                };
            }
        }

        if !status.is_success() {
            return Err(CopilotError::OAuth(format!(
                "Token request failed: {} - {}",
                status.as_u16(),
                text
            )));
        }

        let token: OAuthTokenResponse = serde_json::from_str(&text)?;
        Ok(token)
    }

    /// Complete the full device flow with automatic polling
    ///
    /// This is a convenience method that handles polling automatically.
    /// The callback is invoked with the user code and verification URI
    /// so the caller can display them to the user.
    pub async fn complete_device_flow<F>(
        &self,
        on_code: F,
    ) -> CopilotResult<OAuthTokenResponse>
    where
        F: FnOnce(&str, &str),
    {
        let device_code = self.start_device_flow().await?;

        // Notify caller of the code
        on_code(&device_code.user_code, &device_code.verification_uri);

        let interval = Duration::from_secs(device_code.interval.max(5));
        let deadline = Instant::now() + Duration::from_secs(device_code.expires_in);

        while Instant::now() < deadline {
            tokio::time::sleep(interval).await;

            match self.poll_for_token(&device_code).await {
                Ok(token) => return Ok(token),
                Err(CopilotError::AuthorizationPending) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(CopilotError::DeviceCodeExpired)
    }
}

/// GitHub Copilot API client with automatic token refresh
#[derive(Clone)]
pub struct CopilotClient {
    http: reqwest::Client,
    oauth_token: String,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl std::fmt::Debug for CopilotClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopilotClient")
            .field("oauth_token", &"[REDACTED]")
            .finish()
    }
}

impl CopilotClient {
    /// Create a new Copilot client with an OAuth token
    ///
    /// The OAuth token should be obtained via [`CopilotAuth`] or loaded
    /// from persistent storage.
    pub fn new(oauth_token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            oauth_token: oauth_token.into(),
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the OAuth token (for persistence)
    pub fn oauth_token(&self) -> &str {
        &self.oauth_token
    }

    /// Exchange OAuth token for Copilot API token
    async fn get_copilot_token(&self) -> CopilotResult<CachedToken> {
        let response = self
            .http
            .get(COPILOT_TOKEN_URL)
            .header("Authorization", format!("token {}", self.oauth_token))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(CopilotError::TokenExchange(format!(
                "Token exchange failed: {} - {}",
                status, text
            )));
        }

        let token_response: CopilotTokenResponse = response.json().await?;

        // Calculate expiration time
        let expires_at = Instant::now()
            + Duration::from_secs(
                (token_response.expires_at - chrono::Utc::now().timestamp())
                    .max(0) as u64,
            );

        let api_base = if token_response.endpoints.api.is_empty() {
            COPILOT_API_BASE.to_string()
        } else {
            token_response.endpoints.api
        };

        Ok(CachedToken {
            token: token_response.token,
            expires_at,
            api_base,
        })
    }

    /// Get a valid API token, refreshing if necessary
    async fn ensure_token(&self) -> CopilotResult<(String, String)> {
        // Fast path: check cache with read lock
        {
            let cache = self.cached_token.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_expired() {
                    return Ok((cached.token.clone(), cached.api_base.clone()));
                }
            }
        }

        // Slow path: acquire write lock and double-check
        let mut cache = self.cached_token.write().await;

        // Re-check under write lock (another task may have refreshed)
        if let Some(ref cached) = *cache {
            if !cached.is_expired() {
                return Ok((cached.token.clone(), cached.api_base.clone()));
            }
        }

        // Refresh token while holding the lock
        let new_token = self.get_copilot_token().await?;
        let result = (new_token.token.clone(), new_token.api_base.clone());
        *cache = Some(new_token);

        Ok(result)
    }

    /// List available models
    pub async fn list_models(&self) -> CopilotResult<Vec<CopilotModel>> {
        let (token, api_base) = self.ensure_token().await?;

        let response = self
            .http
            .get(format!("{}/models", api_base))
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(CopilotError::Api {
                status,
                message: text,
            });
        }

        let models: ModelsResponse = response.json().await?;
        Ok(models.data)
    }

    /// Get the API base URL (for use with OpenAI-compatible clients)
    pub async fn api_base(&self) -> CopilotResult<String> {
        let (_, api_base) = self.ensure_token().await?;
        Ok(api_base)
    }

    /// Get the current API token (for use with OpenAI-compatible clients)
    ///
    /// Note: This token expires after ~30 minutes. The client automatically
    /// refreshes expired tokens on subsequent calls.
    pub async fn api_token(&self) -> CopilotResult<String> {
        let (token, _) = self.ensure_token().await?;
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(COPILOT_CLIENT_ID, "01ab8ac9400c4e429b23");
        assert!(DEVICE_CODE_URL.starts_with("https://github.com"));
        assert!(COPILOT_API_BASE.starts_with("https://api.githubcopilot.com"));
    }

    #[test]
    fn test_device_code_deserialize() {
        let json = r#"{
            "device_code": "abc123",
            "user_code": "ABCD-1234",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        }"#;

        let response: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.device_code, "abc123");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.interval, 5);
    }

    #[test]
    fn test_oauth_token_deserialize() {
        let json = r#"{
            "access_token": "gho_xxxxxxxxxxxx",
            "token_type": "bearer",
            "scope": "user:email"
        }"#;

        let response: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert!(response.access_token.starts_with("gho_"));
        assert_eq!(response.token_type, "bearer");
    }

    #[test]
    fn test_copilot_client_debug_redacts_token() {
        let client = CopilotClient::new("gho_secret_token");
        let debug = format!("{:?}", client);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("gho_secret_token"));
    }
}
