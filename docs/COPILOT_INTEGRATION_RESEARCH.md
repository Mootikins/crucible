# GitHub Copilot Chat Integration for Crucible - Research & Implementation Guide

## 🎯 Overview

This document outlines how to integrate GitHub Copilot's chat/agent capabilities into Crucible for IP-secure, enterprise-approved AI assistance within your internal repository constraints.

## 📋 Key Findings

### What GitHub Copilot Offers for Chat/Agents

1. **Official GitHub Copilot Extensions API** ✅ Recommended
   - Purpose-built for creating custom agents integrated with Copilot Chat
   - Uses OAuth for secure authentication (no API key exposure)
   - Your code stays within GitHub's infrastructure (IP security)
   - Supports function calling for custom integrations

2. **Authentication Methods**
   - **OAuth (Official)** - One-click authentication, tokens stored locally
   - **Personal Access Token (PAT)** - Alternative for programmatic access (requires `manage_billing:copilot` or `read:org` scopes)
   - **GitHub App OAuth** - Used by Extensions

3. **Limitations**
   - No direct public API for Copilot Chat (by design - for security/privacy)
   - Undocumented APIs exist but aren't officially supported
   - Must be used through official channels or IDE extensions

## 🏗️ Implementation Architecture

### High-Level Flow for Crucible

```
User in VS Code (or other IDE)
    ↓
Types "@crucible [question]"
    ↓
Copilot Chat identifies your Crucible extension
    ↓
Sends message to Crucible Agent Server (HTTP endpoint)
    ↓
Agent processes request:
  1. Authenticates with GitHub OAuth
  2. Calls Crucible's MCP tools (semantic_search, text_search, etc.)
  3. Uses GitHub Copilot's LLM API for text generation
  4. Returns response to Copilot Chat
    ↓
Response appears in VS Code Copilot Chat panel
```

### Two-Tier Implementation Approach

#### Option A: Lightweight - "Just Chat"
- Deploy simple HTTP server that acts as Copilot agent
- Receives chat messages from Copilot Chat
- Uses Copilot's LLM API to generate responses
- Integrates with Crucible's existing search/tools via MCP
- Users see Crucible as a Copilot extension

**Complexity:** Medium | **Time:** 1-2 weeks | **Maintenance:** Low

#### Option B: Full-Featured - "Agent with Tools"
- Full agent implementation with function calling
- Crucible tools become callable functions within Copilot Chat
- Users can ask Copilot to create notes, search, etc.
- More powerful but more complex

**Complexity:** High | **Time:** 3-4 weeks | **Maintenance:** Medium

## 🔧 Specific Crucible Implementation Steps

### Phase 1: Foundation (Weeks 1-2)

#### 1.1 Create GitHub App & Setup OAuth
```
1. Register GitHub App at https://github.com/settings/apps
2. Configure:
   - Name: "Crucible Copilot Extension"
   - Homepage URL: https://your-domain/crucible
   - Webhook URL: https://your-domain/crucible/webhook
   - Permissions:
     * Read repository contents
     * Read metadata
3. Generate credentials (Client ID, Client Secret)
4. Note the App ID for later
```

#### 1.2 Create New Rust Crate: `crucible-copilot-agent`
```bash
# Create new crate in workspace
cargo new crates/crucible-copilot-agent --lib
```

**Structure:**
```
crucible-copilot-agent/
├── src/
│   ├── lib.rs              # Module organization
│   ├── server.rs           # HTTP server for agent
│   ├── auth.rs             # OAuth token handling
│   ├── copilot_client.rs   # GitHub Copilot API client
│   ├── request_handler.rs  # Process Copilot Chat messages
│   └── tools/
│       ├── mod.rs
│       ├── search.rs       # Integrate Crucible search tools
│       └── notes.rs        # Integrate Crucible note tools
├── Cargo.toml
└── tests/
    └── integration_test.rs
```

#### 1.3 Cargo.toml Dependencies
```toml
[dependencies]
# Core Crucible
crucible-core = { path = "../crucible-core" }
crucible-tools = { path = "../crucible-tools" }
crucible-llm = { path = "../crucible-llm" }

# Web framework for agent server
axum = "0.7"
tokio = { workspace = true, features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["trace", "cors"] }

# GitHub API & OAuth
reqwest = { workspace = true, features = ["json"] }
async-trait = { workspace = true }

# Serialization
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }

# Error handling
anyhow = { workspace = true }
thiserror = { workspace = true }

# Logging
tracing = { workspace = true }

# UUID for conversation tracking
uuid = { workspace = true, features = ["v4", "serde"] }

# JWT/OAuth
jsonwebtoken = "9.2"

# Time
chrono = { workspace = true }
```

### Phase 2: GitHub Copilot Integration (Week 2-3)

#### 2.1 Implement OAuth Token Exchange
```rust
// In auth.rs
pub struct OAuthConfig {
    client_id: String,
    client_secret: String,
    app_id: String,
}

pub async fn exchange_device_code(
    code: String,
    config: &OAuthConfig,
) -> Result<String, AuthError> {
    // Exchange device code for GitHub token
    // Store token locally (or in secure storage)
    // Return github_oauth_token (gho_*)
}

pub async fn get_copilot_chat_token(
    github_token: &str,
) -> Result<String, AuthError> {
    // Exchange github token for Copilot Chat token
    // POST to https://github.com/github-copilot/chat/token
    // Returns short-lived Copilot token
}
```

#### 2.2 Create Copilot API Client
```rust
// In copilot_client.rs
pub struct CopilotLLMClient {
    token: String,  // X-Github-Token header
    http_client: reqwest::Client,
}

impl CopilotLLMClient {
    pub async fn chat_completion(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> Result<String, CopilotError> {
        // POST to https://api.githubcopilot.com/chat/completions
        // OpenAI API format
        // Header: X-Github-Token: <token>
    }
}
```

#### 2.3 Build HTTP Agent Server
```rust
// In server.rs
use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Router,
};

pub async fn start_agent_server(
    port: u16,
    config: CopilotConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/api/chat", post(handle_chat_request))
        .route("/health", get(health_check))
        .with_state(config);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_chat_request(
    Json(request): Json<CopilotChatRequest>,
) -> impl IntoResponse {
    // Parse incoming message from Copilot Chat
    // Enrich context using Crucible search tools
    // Call Copilot LLM API
    // Return response
}
```

### Phase 3: Crucible Integration (Week 3-4)

#### 3.1 Wrap Existing Tools
```rust
// In tools/search.rs
pub struct CrucibleSearchTool {
    search_provider: Arc<dyn SearchProvider>,
}

impl CrucibleSearchTool {
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ToolError> {
        // Call existing crucible-tools semantic_search
        // Format results for Copilot context
    }

    pub async fn text_search(
        &self,
        query: &str,
    ) -> Result<Vec<SearchResult>, ToolError> {
        // Call existing crucible-tools text_search
    }
}
```

#### 3.2 Context Enrichment in Chat Handler
```rust
async fn handle_chat_request(
    Json(request): Json<CopilotChatRequest>,
    State(tools): State<CrucibleTools>,
) -> impl IntoResponse {
    // 1. Extract user query
    let user_query = request.messages.last().unwrap().content.clone();

    // 2. Search Crucible for relevant context
    let search_results = tools
        .semantic_search(&user_query, 5)
        .await
        .unwrap_or_default();

    // 3. Build augmented prompt
    let system_prompt = format!(
        "You are a helpful assistant integrated with a knowledge management system called Crucible.\n\
         Here is relevant context from the user's knowledge base:\n{:?}",
        search_results
    );

    // 4. Call Copilot LLM API with augmented context
    let response = copilot_client
        .chat_completion(request.messages, Some(system_prompt))
        .await;

    // 5. Return response
    Json(response)
}
```

#### 3.3 Deploy Agent Server
- Create standalone binary: `crucible-copilot-agent` CLI
- Or integrate into existing `cru` CLI as a `cru copilot-agent` command
- Deploy to accessible HTTP endpoint (required by GitHub)

### Phase 4: GitHub Registration (Week 4)

#### 4.1 Register with GitHub Copilot Extensions Marketplace
1. Package agent code and configuration
2. Submit to GitHub Copilot Extensions marketplace
3. Users install extension and authenticate

#### 4.2 Configuration Files
```yaml
# crucible-copilot-agent.yml
name: "Crucible Knowledge Agent"
description: "AI-powered access to your Crucible knowledge base"
icon: "https://..."
version: "1.0.0"
repository: "https://github.com/your-org/crucible"
```

## 🔌 Integration Points with Existing Crucible Code

### Reuse Existing Components

**From `crucible-tools` crate:**
- `notes.rs` - Note CRUD operations
- `search.rs` - Semantic & text search
- `kiln.rs` - Knowledge base information
- MCP server infrastructure

**From `crucible-llm` crate:**
- Embedding providers (for context enrichment)
- Text generation interface

**From `crucible-cli` crate:**
- `commands/chat.rs` - Reference for chat architecture
- `agents/` - Existing agent backend patterns

### Create New Integration Layer

```rust
// crucible-copilot-agent/src/crucible_integration.rs
pub struct CrucibleContext {
    tools_provider: Arc<ToolsProvider>,
    embedding_provider: Arc<EmbeddingProvider>,
    search_provider: Arc<SearchProvider>,
}

impl CrucibleContext {
    pub async fn enrich_message(
        &self,
        message: &str,
        conversation_history: &[Message],
    ) -> Result<EnrichedContext, ContextError> {
        // Use semantic search with embeddings
        // Get relevant notes
        // Extract related links (graph traversal)
        // Format as augmentation for Copilot
    }
}
```

## 📊 Deployment Architecture

### Local Development
```
[Your Machine]
├── VS Code with Copilot extension installed
├── Crucible knowledge base
└── crucible-copilot-agent server (localhost:3000)
    └── http://localhost:3000/api/chat
```

**For testing locally:**
- GitHub allows local development servers for testing
- Use ngrok/similar to expose localhost to GitHub if needed

### Production Deployment

**Option 1: Self-Hosted (Recommended for IP Security)**
```
[Your Organization Server]
├── crucible-copilot-agent (REST API)
├── GitHub OAuth credentials (secured)
├── Crucible instance (file system)
└── SurrealDB (embedded or network)
```

**Option 2: Cloud Deployment**
```
[Your Cloud Provider] (AWS/GCP/Azure)
├── Container running crucible-copilot-agent
├── Environment variables for credentials
├── Secrets management (Vault, etc.)
└── TLS/HTTPS endpoint
```

## 🔐 Security Considerations

### IP/Privacy Protection
✅ **Why this works:**
- Code never leaves GitHub infrastructure
- OAuth tokens used only for GitHub authentication
- Chat context enriched locally before sending to Copilot
- Your internal repositories remain private

### Authentication Flow
```
User authenticates → GitHub OAuth → GitHub provides token →
Stored locally/securely → Used for Copilot API calls only
```

### Credential Management
```
Environment Variables:
- GITHUB_COPILOT_CLIENT_ID
- GITHUB_COPILOT_CLIENT_SECRET
- GITHUB_APP_ID

Or: Secure credential store (OS keyring, HashiCorp Vault)
```

## 📈 Future Enhancements

### Phase 2 (After MVP)
- **Function Calling** - Allow Copilot to call Crucible tools directly
  - Create note
  - Update metadata
  - Graph traversal

- **Conversation Memory** - Store chat history in Crucible
  - Link conversations to notes
  - Tag important exchanges

- **Multi-Model Support** - Abstract LLM provider
  - Copilot Chat API
  - Claude (via SDK)
  - GPT-4

- **Advanced RAG** - Full retrieval-augmented generation
  - Reranking
  - Multi-hop reasoning
  - Citation/provenance tracking

## 🚀 Quick Reference: Implementation Checklist

### Week 1-2: Foundation
- [ ] Create GitHub App at github.com/settings/apps
- [ ] Create `crucible-copilot-agent` crate
- [ ] Implement OAuth token exchange
- [ ] Build HTTP server skeleton
- [ ] Unit tests for auth flow

### Week 2-3: Copilot Integration
- [ ] Implement CopilotLLMClient
- [ ] Build chat request handler
- [ ] Test with local Copilot setup
- [ ] Error handling and retry logic
- [ ] Integration tests

### Week 3-4: Crucible Integration
- [ ] Wrap crucible-tools (search, notes)
- [ ] Implement context enrichment
- [ ] Build conversation context manager
- [ ] End-to-end testing
- [ ] Documentation

### Week 4: Registration & Deployment
- [ ] Register with GitHub Extensions marketplace
- [ ] Deploy to production endpoint
- [ ] User testing & feedback
- [ ] Security audit
- [ ] Documentation for users

## 📚 Key Resources

### Official Documentation
- [GitHub Copilot Extensions Quickstart](https://docs.github.com/en/copilot/building-copilot-extensions/quickstart-for-github-copilot-extensions-using-agents)
- [Building Copilot Agents](https://docs.github.com/en/copilot/building-copilot-extensions/building-a-copilot-agent-for-your-copilot-extension)
- [Using Copilot's LLM API](https://docs.github.com/en/copilot/how-tos/use-copilot-extensions/build-a-copilot-agent/use-copilots-llm)

### Example Projects
- [Blackbeard Extension](https://github.com/copilot-extensions/blackbeard-extension) - Simple pirate agent (Node.js reference)
- [RAG Extension](https://github.com/copilot-extensions/rag-extension) - Knowledge base example (Go reference)
- [GitHub Models Agent](https://github.com/copilot-extensions/github-models-extension) - Function calling example

### Related Crucible Code
- `crates/crucible-cli/src/commands/chat.rs` - Existing chat implementation
- `crates/crucible-tools/src/mcp_server.rs` - MCP server setup
- `crates/crucible-llm/src/` - LLM provider patterns

## ❓ FAQ

**Q: Will this work with our IP security requirements?**
A: Yes. Code stays in GitHub infrastructure, no external model providers needed. Perfect for internal repos.

**Q: Do we need the GitHub Copilot Enterprise plan?**
A: No, this works with any Copilot plan (Pro, Business, or Enterprise). Business/Enterprise gets additional features.

**Q: What if we want to self-host?**
A: You deploy your agent server on your infrastructure. Copilot Chat calls your endpoint. You control the data flow.

**Q: Can we run this locally first?**
A: Yes! GitHub allows localhost:3000 for testing during development. Use ngrok to expose locally.

**Q: How much does this cost?**
A: User cost is Copilot subscription (existing). Server hosting cost depends on your deployment (AWS, self-hosted, etc.).

**Q: What about offline usage?**
A: This requires GitHub Copilot (online). For offline, see alternatives like Claude, Ollama, or other open models.

---

**Next Steps:**
1. Review this document with your team
2. Check if you want Option A (lightweight) or Option B (full-featured)
3. Set up GitHub App credentials
4. Begin Phase 1 implementation (crate setup + OAuth)
