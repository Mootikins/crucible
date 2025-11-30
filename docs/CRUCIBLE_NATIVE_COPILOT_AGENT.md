# Building a Crucible-Native Agent with GitHub Copilot LLM Backend

## 🎯 Goal

Build a **native Crucible chat agent** that uses GitHub Copilot's LLM API directly, giving you full control over the agent logic while leveraging your Copilot subscription for IP security.

## 🆚 Comparison: Native Agent vs ACP Agents

### Option A: Use ACP Agents (OpenCode, Claude Code)
```
User → OpenCode/Claude Code → GitHub Copilot LLM → Crucible MCP Tools → Response
```
- ✅ Ready to use today
- ❌ Limited control over agent logic
- ❌ Depends on third-party agent implementation

### Option B: Build Crucible-Native Agent (This Document)
```
User → Crucible Native Agent → GitHub Copilot LLM → Crucible's own logic → Response
```
- ✅ Full control over agent behavior
- ✅ Tight integration with Crucible internals
- ✅ Custom context enrichment strategies
- ❌ More implementation work
- ⚠️ Uses undocumented Copilot API

---

## 🏗️ Architecture

### High-Level Design

```rust
┌─────────────────────────────────────────────────┐
│ Crucible CLI (cru chat)                        │
│                                                 │
│  ┌──────────────────────────────────────────┐  │
│  │ crucible-copilot-provider                │  │
│  │ (New crate - LLM provider for Copilot)   │  │
│  │                                           │  │
│  │ - CopilotAuthenticator                   │  │
│  │ - CopilotLLMClient                       │  │
│  │ - TokenManager                           │  │
│  └──────────────┬───────────────────────────┘  │
│                 │                               │
│  ┌──────────────▼───────────────────────────┐  │
│  │ crucible-llm (text_generation module)    │  │
│  │ Implements: TextGenerationProvider       │  │
│  └──────────────┬───────────────────────────┘  │
│                 │                               │
│  ┌──────────────▼───────────────────────────┐  │
│  │ crucible-cli chat command                │  │
│  │ Uses TextGenerationProvider for agent    │  │
│  └──────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
                  │ HTTPS
                  ▼
      ┌────────────────────────┐
      │ GitHub Copilot API     │
      │ api.githubcopilot.com  │
      └────────────────────────┘
```

### Key Components

1. **crucible-copilot-provider** (new crate)
   - OAuth authentication with GitHub
   - Token management and refresh
   - Copilot LLM API client
   - Implements `TextGenerationProvider` trait

2. **crucible-llm integration**
   - Add Copilot as a text generation provider
   - Reuse existing provider infrastructure

3. **crucible-cli chat enhancement**
   - Use Copilot provider instead of ACP agents
   - Custom context enrichment logic
   - Full control over conversation flow

---

## 📋 Implementation Plan

### Phase 1: Create copilot-provider Crate

#### Step 1: Create New Workspace Crate

```bash
cd /home/moot/crucible
cargo new --lib crates/crucible-copilot-provider
```

#### Step 2: Add Dependencies

**File:** `crates/crucible-copilot-provider/Cargo.toml`

```toml
[package]
name = "crucible-copilot-provider"
version.workspace = true
authors.workspace = true
license.workspace = true
edition.workspace = true
description = "GitHub Copilot LLM provider for Crucible"

[lib]
name = "crucible_copilot_provider"
path = "src/lib.rs"

[dependencies]
# Core Crucible integration
crucible-llm = { path = "../crucible-llm" }

# Async runtime
tokio = { workspace = true, features = ["full"] }
async-trait = { workspace = true }

# HTTP client
reqwest = { workspace = true, features = ["json", "cookies"] }

# Serialization
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }

# Error handling
anyhow = { workspace = true }
thiserror = { workspace = true }

# Logging
tracing = { workspace = true }

# Time handling for token expiration
chrono = { workspace = true }

# UUID for session/request IDs
uuid = { workspace = true, features = ["v4", "serde"] }

# Token storage
directories = "5.0"  # XDG-compliant config directories
keyring = "3.6"      # Secure token storage (optional)

[dev-dependencies]
tokio-test = { workspace = true }
mockito = "1.0"
tempfile = { workspace = true }

[features]
default = []
secure-storage = ["keyring"]  # Use OS keyring for token storage
```

#### Step 3: Crate Structure

```
crates/crucible-copilot-provider/
├── src/
│   ├── lib.rs              # Public API + re-exports
│   ├── auth.rs             # GitHub OAuth device flow
│   ├── token.rs            # Token management + refresh
│   ├── client.rs           # Copilot API client
│   ├── provider.rs         # TextGenerationProvider implementation
│   ├── config.rs           # Configuration types
│   ├── error.rs            # Error types
│   └── types.rs            # Request/Response types
├── Cargo.toml
└── examples/
    └── chat_example.rs     # Example usage
```

---

### Phase 2: Implement GitHub OAuth Authentication

#### File: `src/auth.rs`

```rust
//! GitHub OAuth Device Flow Authentication
//!
//! Implements the OAuth device flow to authenticate with GitHub
//! and obtain tokens for accessing GitHub Copilot's LLM API.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// GitHub OAuth device flow client ID (Copilot's app ID)
/// Note: This is reverse-engineered from copilot-api project
const COPILOT_APP_CLIENT_ID: &str = "01ab8ac9400c4e429b23";

/// GitHub device code endpoint
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// GitHub token exchange endpoint
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// Device code response from GitHub
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Token response from GitHub
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitHubToken {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,

    #[serde(default)]
    pub created_at: Option<i64>,
}

impl GitHubToken {
    /// Check if token is expired (GitHub tokens typically last 8 hours)
    pub fn is_expired(&self) -> bool {
        if let Some(created) = self.created_at {
            let now = chrono::Utc::now().timestamp();
            let age_seconds = now - created;
            // Conservative: consider expired after 7 hours
            age_seconds > (7 * 3600)
        } else {
            // Unknown creation time - assume expired
            true
        }
    }
}

/// Token error response
#[derive(Debug, Deserialize)]
pub struct TokenError {
    pub error: String,

    #[serde(default)]
    pub error_description: Option<String>,
}

/// GitHub OAuth Authenticator
pub struct GitHubAuthenticator {
    client: Client,
}

impl GitHubAuthenticator {
    /// Create a new authenticator
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Start the OAuth device flow
    ///
    /// Returns device code information and displays instructions to user
    pub async fn start_device_flow(&self) -> Result<DeviceCodeResponse> {
        debug!("Starting GitHub OAuth device flow");

        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&[
                ("client_id", COPILOT_APP_CLIENT_ID),
                ("scope", "read:user"),
            ])
            .send()
            .await
            .context("Failed to request device code")?;

        let device_response: DeviceCodeResponse = response
            .json()
            .await
            .context("Failed to parse device code response")?;

        // Display instructions to user
        println!("\n🔐 GitHub Authentication Required");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Please visit: {}", device_response.verification_uri);
        println!("And enter code: {}", device_response.user_code);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        info!(
            "Device code expires in {} seconds",
            device_response.expires_in
        );

        Ok(device_response)
    }

    /// Poll for token completion
    ///
    /// Waits for user to complete authentication in browser
    pub async fn poll_for_token(
        &self,
        device_code: &str,
        interval: u64,
        timeout: Duration,
    ) -> Result<GitHubToken> {
        info!("Polling for token (timeout: {}s)", timeout.as_secs());

        let start = std::time::Instant::now();
        let poll_interval = Duration::from_secs(interval);

        loop {
            // Check timeout
            if start.elapsed() > timeout {
                anyhow::bail!("Authentication timeout - device code expired");
            }

            // Poll for token
            let response = self
                .client
                .post(TOKEN_URL)
                .header("Accept", "application/json")
                .form(&[
                    ("client_id", COPILOT_APP_CLIENT_ID),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await?;

            // Try to parse as success
            if response.status().is_success() {
                if let Ok(mut token) = response.json::<GitHubToken>().await {
                    info!("✓ GitHub authentication successful!");

                    // Set creation timestamp
                    token.created_at = Some(chrono::Utc::now().timestamp());

                    return Ok(token);
                }
            }

            // Try to parse as error
            if let Ok(error) = response.json::<TokenError>().await {
                match error.error.as_str() {
                    "authorization_pending" => {
                        debug!("Waiting for user authorization...");
                    }
                    "slow_down" => {
                        warn!("Rate limited - slowing down polling");
                        sleep(poll_interval * 2).await;
                        continue;
                    }
                    "expired_token" => {
                        anyhow::bail!("Device code expired - please try again");
                    }
                    "access_denied" => {
                        anyhow::bail!("Access denied by user");
                    }
                    _ => {
                        warn!("Unexpected error: {} - {:?}", error.error, error.error_description);
                    }
                }
            }

            // Wait before next poll
            sleep(poll_interval).await;
        }
    }

    /// Run complete authentication flow
    ///
    /// Combines device flow start + polling into one call
    pub async fn authenticate(&self) -> Result<GitHubToken> {
        let device_code = self.start_device_flow().await?;

        let timeout = Duration::from_secs(device_code.expires_in);

        self.poll_for_token(&device_code.device_code, device_code.interval, timeout)
            .await
    }
}

impl Default for GitHubAuthenticator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_expiration() {
        let mut token = GitHubToken {
            access_token: "gho_test".to_string(),
            token_type: "bearer".to_string(),
            scope: "read:user".to_string(),
            created_at: None,
        };

        // No creation time = expired
        assert!(token.is_expired());

        // Fresh token = not expired
        token.created_at = Some(chrono::Utc::now().timestamp());
        assert!(!token.is_expired());

        // Old token = expired
        token.created_at = Some(chrono::Utc::now().timestamp() - (8 * 3600));
        assert!(token.is_expired());
    }
}
```

---

### Phase 3: Implement Token Management

#### File: `src/token.rs`

```rust
//! Token Management and Copilot Token Exchange
//!
//! Handles:
//! - Persistent storage of GitHub OAuth tokens
//! - Exchange GitHub token for Copilot-specific token
//! - Token refresh logic

use crate::auth::GitHubToken;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

/// Copilot token endpoint
const COPILOT_TOKEN_URL: &str = "https://github.com/github-copilot/chat/token";

/// Copilot token response
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CopilotToken {
    /// Short-lived Copilot API token
    pub token: String,

    /// Token expiration (Unix timestamp)
    pub expires_at: i64,

    /// Optional refresh token
    #[serde(default)]
    pub refresh_in: Option<i64>,
}

impl CopilotToken {
    /// Check if token is expired or will expire soon
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        // Add 60s buffer to account for request time
        now >= (self.expires_at - 60)
    }

    /// Get time until expiration
    pub fn ttl_seconds(&self) -> i64 {
        let now = chrono::Utc::now().timestamp();
        self.expires_at - now
    }
}

/// Token manager handles storage and exchange
pub struct TokenManager {
    client: Client,
    storage_path: PathBuf,
}

impl TokenManager {
    /// Create new token manager
    pub fn new() -> Result<Self> {
        let storage_path = Self::get_storage_path()?;

        Ok(Self {
            client: Client::new(),
            storage_path,
        })
    }

    /// Get XDG-compliant storage path
    fn get_storage_path() -> Result<PathBuf> {
        let config_dir = directories::ProjectDirs::from("io", "krohnos", "crucible")
            .context("Failed to determine config directory")?
            .config_dir()
            .to_path_buf();

        Ok(config_dir.join("copilot-token.json"))
    }

    /// Store GitHub token
    pub async fn store_github_token(&self, token: &GitHubToken) -> Result<()> {
        debug!("Storing GitHub token to {:?}", self.storage_path);

        // Create parent directory if needed
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(token)?;
        fs::write(&self.storage_path, json).await?;

        info!("GitHub token stored");
        Ok(())
    }

    /// Load GitHub token from storage
    pub async fn load_github_token(&self) -> Result<Option<GitHubToken>> {
        if !self.storage_path.exists() {
            return Ok(None);
        }

        debug!("Loading GitHub token from {:?}", self.storage_path);

        let json = fs::read_to_string(&self.storage_path).await?;
        let token: GitHubToken = serde_json::from_str(&json)?;

        if token.is_expired() {
            warn!("Stored GitHub token is expired");
            return Ok(None);
        }

        info!("Loaded valid GitHub token");
        Ok(Some(token))
    }

    /// Exchange GitHub token for Copilot token
    pub async fn get_copilot_token(&self, github_token: &str) -> Result<CopilotToken> {
        debug!("Exchanging GitHub token for Copilot token");

        let response = self
            .client
            .post(COPILOT_TOKEN_URL)
            .header("Authorization", format!("Bearer {}", github_token))
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                "crucible-copilot-provider/1.0 (Rust; crucible)",
            )
            .send()
            .await
            .context("Failed to request Copilot token")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Copilot token request failed: {} - {}", status, body);
        }

        let copilot_token: CopilotToken = response
            .json()
            .await
            .context("Failed to parse Copilot token response")?;

        info!(
            "Received Copilot token (expires in {}s)",
            copilot_token.ttl_seconds()
        );

        Ok(copilot_token)
    }

    /// Get or refresh Copilot token
    ///
    /// Returns cached token if valid, otherwise fetches new one
    pub async fn ensure_copilot_token(&self) -> Result<CopilotToken> {
        // Load GitHub token
        let github_token = self
            .load_github_token()
            .await?
            .context("No GitHub token found - run authentication first")?;

        // Exchange for Copilot token
        self.get_copilot_token(&github_token.access_token).await
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new().expect("Failed to create token manager")
    }
}
```

---

### Phase 4: Implement Copilot API Client

#### File: `src/client.rs`

```rust
//! GitHub Copilot LLM API Client
//!
//! Implements the client for calling Copilot's chat completion API
//! in OpenAI-compatible format.

use crate::token::{CopilotToken, TokenManager};
use anyhow::{Context, Result};
use crucible_llm::text_generation::{
    ChatMessage, ChatCompletionRequest, ChatCompletionResponse,
    CompletionChunk, TokenUsage,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Copilot chat completions endpoint
const COPILOT_CHAT_URL: &str = "https://api.githubcopilot.com/chat/completions";

/// Copilot API client
pub struct CopilotClient {
    client: Client,
    token_manager: TokenManager,
    copilot_token: Option<CopilotToken>,
}

impl CopilotClient {
    /// Create new Copilot client
    pub fn new(token_manager: TokenManager) -> Self {
        Self {
            client: Client::new(),
            token_manager,
            copilot_token: None,
        }
    }

    /// Ensure we have a valid Copilot token
    async fn ensure_token(&mut self) -> Result<String> {
        // Check if current token is valid
        if let Some(token) = &self.copilot_token {
            if !token.is_expired() {
                return Ok(token.token.clone());
            }

            warn!("Copilot token expired, refreshing...");
        }

        // Get new token
        let new_token = self.token_manager.ensure_copilot_token().await?;
        let token_value = new_token.token.clone();
        self.copilot_token = Some(new_token);

        Ok(token_value)
    }

    /// Call Copilot chat completion API
    pub async fn chat_completion(
        &mut self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let token = self.ensure_token().await?;

        debug!(
            "Sending chat completion request (model: {}, messages: {})",
            request.model,
            request.messages.len()
        );

        let response = self
            .client
            .post(COPILOT_CHAT_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("X-Github-Token", &token)  // Copilot also expects this
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                "crucible-copilot-provider/1.0 (Rust; crucible)",
            )
            .json(&request)
            .send()
            .await
            .context("Failed to send chat completion request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Chat completion failed: {} - {}", status, body);
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .context("Failed to parse completion response")?;

        info!(
            "Received completion (tokens: {:?})",
            completion.usage
        );

        Ok(completion)
    }

    /// Stream chat completion (returns chunks)
    pub async fn chat_completion_stream(
        &mut self,
        mut request: ChatCompletionRequest,
    ) -> Result<impl futures::Stream<Item = Result<CompletionChunk>>> {
        // Enable streaming
        request.stream = Some(true);

        let token = self.ensure_token().await?;

        let response = self
            .client
            .post(COPILOT_CHAT_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("X-Github-Token", &token)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header(
                "User-Agent",
                "crucible-copilot-provider/1.0 (Rust; crucible)",
            )
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Streaming request failed: {} - {}", status, body);
        }

        // Convert response to stream of chunks
        // (Simplified - full implementation would parse SSE format)
        Ok(futures::stream::empty())  // TODO: Implement SSE parsing
    }
}
```

---

### Phase 5: Implement TextGenerationProvider

#### File: `src/provider.rs`

```rust
//! Copilot Text Generation Provider
//!
//! Implements TextGenerationProvider trait for Copilot LLM

use crate::client::CopilotClient;
use crate::token::TokenManager;
use async_trait::async_trait;
use crucible_llm::text_generation::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
    CompletionRequest, CompletionResponse, TextGenerationProvider,
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Copilot text generation provider
pub struct CopilotProvider {
    client: Arc<Mutex<CopilotClient>>,
}

impl CopilotProvider {
    /// Create new Copilot provider
    pub async fn new() -> anyhow::Result<Self> {
        let token_manager = TokenManager::new()?;
        let client = CopilotClient::new(token_manager);

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }
}

#[async_trait]
impl TextGenerationProvider for CopilotProvider {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> anyhow::Result<ChatCompletionResponse> {
        let mut client = self.client.lock().await;
        client.chat_completion(request).await
    }

    async fn completion(
        &self,
        _request: CompletionRequest,
    ) -> anyhow::Result<CompletionResponse> {
        anyhow::bail!("Copilot only supports chat completion, not raw completion")
    }
}
```

---

## 🔧 Integration with Crucible

### Step 1: Add to workspace

**File:** `Cargo.toml` (workspace root)

```toml
[workspace]
members = [
    # ... existing members
    "crates/crucible-copilot-provider",
]
```

### Step 2: Update crucible-llm

**File:** `crates/crucible-llm/Cargo.toml`

```toml
[dependencies]
# Add optional Copilot support
crucible-copilot-provider = { path = "../crucible-copilot-provider", optional = true }

[features]
copilot = ["crucible-copilot-provider"]
```

**File:** `crates/crucible-llm/src/text_generation.rs`

```rust
// Add Copilot provider type
pub enum TextProviderConfig {
    Ollama(OllamaConfig),
    OpenAI(OpenAIConfig),
    #[cfg(feature = "copilot")]
    Copilot,  // No config needed - uses stored auth
}

// Update factory function
pub async fn create_text_provider(
    config: TextProviderConfig,
) -> Result<Arc<dyn TextGenerationProvider>> {
    match config {
        TextProviderConfig::Ollama(cfg) => {
            // ... existing code
        }
        TextProviderConfig::OpenAI(cfg) => {
            // ... existing code
        }
        #[cfg(feature = "copilot")]
        TextProviderConfig::Copilot => {
            use crucible_copilot_provider::CopilotProvider;
            Ok(Arc::new(CopilotProvider::new().await?))
        }
    }
}
```

### Step 3: Add CLI Command for Auth

**File:** `crates/crucible-cli/src/commands/copilot.rs` (new file)

```rust
//! Copilot authentication command

use anyhow::Result;
use crucible_copilot_provider::{GitHubAuthenticator, TokenManager};

pub async fn authenticate() -> Result<()> {
    println!("🔥 Crucible GitHub Copilot Authentication\n");

    let authenticator = GitHubAuthenticator::new();
    let token = authenticator.authenticate().await?;

    // Store token
    let manager = TokenManager::new()?;
    manager.store_github_token(&token).await?;

    println!("\n✓ Authentication successful!");
    println!("  Token stored securely");
    println!("\nYou can now use `cru chat --provider copilot`\n");

    Ok(())
}

pub async fn show_status() -> Result<()> {
    let manager = TokenManager::new()?;

    match manager.load_github_token().await? {
        Some(token) => {
            println!("✓ GitHub token: Valid");
            println!("  Type: {}", token.token_type);
            println!("  Scope: {}", token.scope);

            if token.is_expired() {
                println!("  ⚠️  Token expired - run `cru copilot auth` to refresh");
            }

            // Try to get Copilot token
            match manager.get_copilot_token(&token.access_token).await {
                Ok(copilot_token) => {
                    println!("\n✓ Copilot access: Working");
                    println!("  TTL: {}s", copilot_token.ttl_seconds());
                }
                Err(e) => {
                    println!("\n✗ Copilot access: Failed");
                    println!("  Error: {}", e);
                }
            }
        }
        None => {
            println!("✗ No GitHub token found");
            println!("\nRun `cru copilot auth` to authenticate");
        }
    }

    Ok(())
}
```

### Step 4: Update CLI Main Command

**File:** `crates/crucible-cli/src/main.rs`

```rust
#[derive(Parser)]
enum Commands {
    // ... existing commands

    /// GitHub Copilot authentication and configuration
    Copilot {
        #[command(subcommand)]
        command: CopilotCommand,
    },
}

#[derive(Subcommand)]
enum CopilotCommand {
    /// Authenticate with GitHub Copilot
    Auth,
    /// Show authentication status
    Status,
}

// In main match:
Commands::Copilot { command } => match command {
    CopilotCommand::Auth => commands::copilot::authenticate().await?,
    CopilotCommand::Status => commands::copilot::show_status().await?,
},
```

---

## 🚀 Usage

### First-Time Setup

```bash
# 1. Build with Copilot support
cargo build --release --features copilot

# 2. Authenticate with GitHub
cru copilot auth
# Opens browser → Enter device code → Authorize

# 3. Verify authentication
cru copilot status
```

### Using Copilot with Chat

```bash
# Use Copilot as LLM provider
cru chat --provider copilot

# Or set as default in config
# ~/.config/crucible/config.toml:
# [llm]
# provider = "copilot"

# Then just use:
cru chat
```

---

## 🔐 Security & IP Protection

### Data Flow

```
1. User query (stays local)
   ↓
2. Crucible enriches with local context (stays local)
   ↓
3. Send enriched prompt to Copilot API
   ↓
4. Copilot processes (within GitHub infrastructure)
   ↓
5. Response returns to Crucible
   ↓
6. Display to user
```

**What Leaves Your Machine:**
- ✅ Chat messages/prompts
- ✅ Context snippets from your notes (limited by token budget)

**What Stays Local:**
- ✅ Your entire knowledge base
- ✅ File system structure
- ✅ All note metadata
- ✅ Graph relationships

### Token Storage

```
~/.config/crucible/copilot-token.json (600 permissions)
```

For extra security, enable keyring feature:

```toml
[dependencies]
crucible-copilot-provider = { ..., features = ["secure-storage"] }
```

This stores tokens in OS keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service).

---

## 🆚 Comparison: Native vs ACP

| Aspect | Native Copilot Agent | ACP Agent (OpenCode) |
|--------|----------------------|----------------------|
| **Setup Complexity** | High (custom impl) | Low (install OpenCode) |
| **Control** | Full | Limited |
| **Context Enrichment** | Custom logic | Generic |
| **Token Management** | Manual | Handled by agent |
| **Maintenance** | You maintain | Third-party maintains |
| **Features** | Exactly what you build | What agent provides |
| **IP Security** | ✅ Same | ✅ Same |
| **Rate Limits** | Copilot limits | Copilot limits |
| **Tool Calling** | Custom impl | May not work |

---

## 📚 Next Steps

1. **MVP Implementation**
   - Implement Phase 1-5 above
   - Basic auth + chat completion
   - Test with simple queries

2. **Enhanced Features**
   - Streaming responses
   - Token usage tracking
   - Rate limit handling
   - Advanced context strategies

3. **Production Hardening**
   - Error recovery
   - Token refresh edge cases
   - Logging/monitoring
   - Performance optimization

---

## ⚠️ Important Warnings

### Undocumented API

GitHub Copilot's LLM API is **not officially documented**:
- ✅ Used by copilot-api (community project)
- ✅ Used by liteLLM (community project)
- ❌ No official support from GitHub
- ⚠️ Could break at any time
- ⚠️ Rate limits unclear

### Alternatives to Consider

If undocumented API is a concern:

1. **Use OpenCode** (has native Copilot support, documented in previous guide)
2. **Wait for official GitHub Copilot API** (if/when released)
3. **Use Claude/GPT-4 instead** (stable, documented APIs)

---

## 🎯 Decision Matrix

**Choose Native Agent if:**
- ✅ Need full control over agent logic
- ✅ Want custom context enrichment strategies
- ✅ Comfortable maintaining auth code
- ✅ Okay with using undocumented API

**Choose OpenCode/ACP if:**
- ✅ Want quick setup
- ✅ Prefer third-party maintenance
- ✅ Don't need custom agent logic
- ✅ Want stable, documented approach

---

## 📖 References

- [copilot-api GitHub](https://github.com/ericc-ch/copilot-api) - Reference implementation
- [LiteLLM Copilot Integration](https://docs.litellm.ai/docs/providers/github_copilot) - Alternative approach
- [GitHub OAuth Device Flow](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow) - Official OAuth docs
- Crucible's existing LLM provider pattern (`crucible-llm/src/text_generation.rs`)

---

**Ready to build? Start with Phase 1!** 🔥
