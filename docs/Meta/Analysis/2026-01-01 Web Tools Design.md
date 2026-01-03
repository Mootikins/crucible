# Web Tools Design for Crucible

**Date**: 2026-01-01
**Status**: Draft
**Crate**: `crucible-scrape`
**Authors**: Research synthesis from OpenCode, Claude Code, Jina, Crawl4AI analysis

## Overview

This document outlines the design for adding web fetch and web search capabilities to Crucible, enabling agents to retrieve and process web content during conversations.

### Design Principles

1. **Local-first**: Prefer local processing over external APIs
2. **Batteries-included**: Core functionality works without API keys
3. **Graceful fallback**: Escalate to more capable (but heavier) backends when needed
4. **Provider-agnostic**: Support multiple search backends via configuration
5. **Explicit persistence**: Fetched content cached in session, explicit action to save to kiln

---

## Part 1: Web Fetch Tool

### Purpose

Convert web URLs into LLM-friendly markdown for use in agent conversations and knowledge ingestion.

### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                          web_fetch tool                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  URL ──► Tier 1: reqwest ──► htmd (HTML→MD) ──► return             │
│              │                     │                                │
│              │ JS required?        │ conversion failed?             │
│              ▼                     ▼                                │
│      Tier 2: chromiumoxide ──► htmd ──► return                     │
│              │                     │                                │
│              │ complex layout?     │ poor quality?                  │
│              ▼                     ▼                                │
│      Tier 3: chromiumoxide ──► ReaderLM-v2 (local) ──► return      │
│              │                                                      │
│              │ model unavailable?                                   │
│              ▼                                                      │
│      Tier 4: Jina r.jina.ai API (fallback)                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Tier Descriptions

| Tier | Components | Use Case | Latency | Dependencies |
|------|------------|----------|---------|--------------|
| **1** | reqwest + htmd | Static HTML pages | ~100ms | None |
| **2** | chromiumoxide + htmd | JS-rendered SPAs | ~2-5s | Chromium binary |
| **3** | chromiumoxide + ReaderLM-v2 | Complex layouts, tables | ~5-15s | Chromium + GGUF model |
| **4** | Jina API | Ultimate fallback | ~1-3s | Internet + API key (optional) |

### Rust Dependencies

```toml
[dependencies]
# HTTP client (already in workspace)
reqwest = { version = "0.12", features = ["json", "gzip", "brotli"] }

# HTML to Markdown conversion
htmd = "0.1"                    # Turndown.js inspired

# Headless browser (optional feature)
chromiumoxide = { version = "0.8", optional = true }

# HTML parsing for content extraction
scraper = "0.21"               # CSS selector-based extraction
html5ever = "0.29"             # HTML5 parser
```

### Tool Interface

The tool supports two modes:
1. **Content mode** (no prompt): Returns full markdown, cached for re-query
2. **Query mode** (with prompt): Returns focused answer, raw content cached

```rust
#[derive(Deserialize, JsonSchema)]
pub struct WebFetchParams {
    /// URL to fetch (must be http:// or https://)
    pub url: String,

    /// Optional question to answer from the page content.
    /// If provided, returns a focused answer instead of full content.
    /// Raw markdown is still cached for follow-up queries.
    pub prompt: Option<String>,

    /// Output format (only applies when prompt is None)
    #[serde(default = "default_format")]
    pub format: FetchFormat,

    /// Maximum content length in characters (default: 100_000)
    #[serde(default = "default_max_length")]
    pub max_length: usize,

    /// Timeout in seconds (default: 30, max: 120)
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Force browser rendering even for static pages
    #[serde(default)]
    pub force_browser: bool,

    /// Use ReaderLM for conversion (requires model)
    #[serde(default)]
    pub use_reader_model: bool,
}

/// Response varies based on whether prompt was provided
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub enum WebFetchResponse {
    /// Full content returned when no prompt provided
    Content {
        url: String,
        title: Option<String>,
        content: String,
        cached_path: PathBuf,
    },
    /// Focused answer when prompt provided
    Answer {
        url: String,
        answer: String,
        /// Path to full cached content for follow-up
        cached_path: PathBuf,
    },
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum FetchFormat {
    #[default]
    Markdown,
    Text,
    Html,
}
```

### Query Mode Processing

When `prompt` is provided, a **dedicated small model** processes the content to extract a focused answer. This follows Claude Code's pattern of using a lightweight model (Haiku) for content summarization.

**Benefits:**
- Reduces context usage in main conversation
- Faster response for simple questions
- Isolates content processing from main agent

**Model options** (configurable):
| Model | Size | Speed | Quality |
|-------|------|-------|---------|
| Qwen2.5-0.5B | 500MB | Fast | Basic |
| Qwen2.5-1.5B | 1.5GB | Medium | Good |
| Phi-3-mini | 2.3GB | Medium | Good |
| Llama-3.2-1B | 1.2GB | Fast | Good |

```rust
pub struct QueryProcessor {
    /// Small model for content Q&A
    model: SmallLanguageModel,

    /// Maximum input tokens (truncate if needed)
    max_input_tokens: usize,

    /// Maximum answer length
    max_output_tokens: usize,
}

impl QueryProcessor {
    pub async fn answer(&self, content: &str, prompt: &str) -> Result<String> {
        let system = "Answer the question based only on the provided content. \
                      Be concise and direct. If the answer isn't in the content, say so.";

        let input = format!("<content>\n{}\n</content>\n\nQuestion: {}", content, prompt);

        self.model.generate(system, &input).await
    }
}
```

### Content Processing Pipeline

```rust
pub struct ContentProcessor {
    /// Heuristic threshold for content quality (0.0-1.0)
    pruning_threshold: f32,

    /// CSS selectors to exclude
    excluded_selectors: Vec<String>,

    /// Minimum word count for valid content
    min_word_count: usize,
}

impl Default for ContentProcessor {
    fn default() -> Self {
        Self {
            pruning_threshold: 0.48,
            min_word_count: 50,
            excluded_selectors: vec![
                "nav", "footer", "aside", "header",
                ".advertisement", ".sidebar", ".comments",
                "script", "style", "noscript",
            ],
        }
    }
}
```

### Caching Strategy

Fetched content flows through three levels:

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Content Lifecycle                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Fetch ──► Session Cache ──► Session Folder ──► Knowledge Base     │
│            (in-memory)       (.crucible/web/)    (deduplicated)     │
│            15 min TTL        persisted           indexed            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**Level 1: In-Memory Cache** (15 minute TTL)
- Fast lookups during active session
- Avoids re-fetching same URL within conversation

**Level 2: Session Folder** (persistent)
- Saved as markdown in `.crucible/web/<url-hash>.md`
- Includes frontmatter: source URL, fetch time, content type
- Available for re-query without re-fetching

**Level 3: Knowledge Base** (explicit save)
- User or agent explicitly saves fetched content to kiln
- Deduplication detects if content already exists
- Saved content indexed and added to semantic search
- Wikilinks can reference saved pages

```rust
pub struct FetchCache {
    /// In-memory cache entries with timestamps
    entries: DashMap<String, CacheEntry>,

    /// Time-to-live for in-memory cache (default: 15 minutes)
    ttl: Duration,

    /// Session folder for persistent cache
    session_path: PathBuf,
}

struct CacheEntry {
    /// Processed markdown content
    markdown: String,

    /// Original HTML (for re-processing)
    raw_html: Option<String>,

    /// Fetch metadata
    fetched_at: Instant,
    content_type: String,
    status_code: u16,

    /// Path to persisted file (if saved)
    persisted_path: Option<PathBuf>,
}
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("URL too long (max 2048 characters)")]
    UrlTooLong,

    #[error("Blocked domain: {0}")]
    BlockedDomain(String),

    #[error("Request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Content too large (max {max} bytes, got {actual})")]
    ContentTooLarge { max: usize, actual: usize },

    #[error("Unsupported content type: {0}")]
    UnsupportedContentType(String),

    #[error("Timeout after {0} seconds")]
    Timeout(u64),

    #[error("Browser error: {0}")]
    BrowserError(String),
}
```

---

## Part 2: ReaderLM-v2 Integration

### Model Specifications

| Attribute | Value |
|-----------|-------|
| **Parameters** | 1.54B |
| **Base** | Qwen2.5-1.5B-Instruct |
| **Context** | 512K tokens (input + output) |
| **Languages** | 29 |
| **License** | CC-BY-NC-4.0 |

### Quantization Options

| Format | Size | RAM/VRAM | Quality | Source |
|--------|------|----------|---------|--------|
| **Q4_K_M** | 986 MB | 8 GB RAM | Good | [rbehzadan/ReaderLM-v2.gguf](https://huggingface.co/rbehzadan/ReaderLM-v2.gguf) |
| **Q8_0** | 1.6 GB | 16 GB RAM / 4 GB VRAM | Excellent | [rbehzadan/ReaderLM-v2.gguf](https://huggingface.co/rbehzadan/ReaderLM-v2.gguf) |
| **BF16** | ~3 GB | 6-8 GB VRAM | Original | [jinaai/ReaderLM-v2](https://huggingface.co/jinaai/ReaderLM-v2) |

### Hardware Requirements

| Workload | Minimum | Recommended |
|----------|---------|-------------|
| **Short pages (<10K tokens)** | 8 GB RAM, Q4 | 16 GB RAM, Q8 |
| **Medium pages (10-50K tokens)** | 16 GB RAM, Q8 | RTX 3060+ (12 GB VRAM) |
| **Long pages (50K+ tokens)** | 32 GB RAM | RTX 3090/4090 (24 GB VRAM) |

**Note**: For long contexts, the KV cache dominates memory usage. A 100K token page may need 16+ GB beyond model weights.

### Integration via llama.cpp

```rust
use llama_cpp_2::{context::params::LlamaContextParams, model::LlamaModel};

pub struct ReaderLMBackend {
    model: LlamaModel,
    ctx_params: LlamaContextParams,
}

impl ReaderLMBackend {
    pub async fn html_to_markdown(&self, html: &str) -> Result<String, BackendError> {
        // ReaderLM-v2 takes raw HTML as input, outputs markdown
        // No special prompt needed - just feed HTML directly
        let tokens = self.model.tokenize(html, true)?;
        let output = self.generate(tokens).await?;
        Ok(output)
    }
}
```

---

## Part 3: Web Search Tool

### Design Goals

1. **Batteries-included**: Works without API keys via DuckDuckGo scraping
2. **Quality tiers**: Better backends available with API keys
3. **Configurable**: Users choose their preferred backend

### Backend Comparison

| Backend | API Key | Cost | Quality | Features |
|---------|---------|------|---------|----------|
| **DuckDuckGo (lite)** | No | Free | Basic | Text search only |
| **SearXNG** | No (self-host) | Free | Good | 243 engines, self-hosted |
| **Brave Search** | Yes | $3/1k | Good | What Claude Code uses |
| **Exa** | Yes | $5/1k | Excellent | Semantic search, MCP server |
| **Tavily** | Yes | $5/1k | Excellent | RAG-optimized |

### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         web_search tool                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Query ──► SearchBackend (configured) ──► Results ──► Summarize    │
│                                                                     │
│  Backends:                                                          │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ DuckDuckGo   │ SearXNG      │ Brave       │ Exa            │   │
│  │ (default)    │ (self-host)  │ (API key)   │ (API key)      │   │
│  │ lite scrape  │ JSON API     │ REST API    │ MCP/REST       │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Recommended Default: DuckDuckGo Lite

For batteries-included operation, scrape DuckDuckGo's lite HTML interface:

```rust
pub struct DuckDuckGoBackend {
    client: reqwest::Client,
    base_url: &'static str, // "https://lite.duckduckgo.com/lite/"
}

impl SearchBackend for DuckDuckGoBackend {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        let response = self.client
            .post(self.base_url)
            .form(&[("q", query)])
            .send()
            .await?;

        let html = response.text().await?;
        self.parse_results(&html, max_results)
    }
}
```

**Pros**: No API key, no rate limits (reasonable use), works immediately
**Cons**: Fragile (HTML structure may change), no semantic search

### Self-Hosted Option: SearXNG

For users who want better quality without API costs:

```rust
pub struct SearXNGBackend {
    client: reqwest::Client,
    instance_url: String, // e.g., "http://localhost:8080"
}

impl SearchBackend for SearXNGBackend {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        let url = format!("{}/search?q={}&format=json", self.instance_url, query);
        let response: SearXNGResponse = self.client.get(&url).send().await?.json().await?;
        Ok(response.results.into_iter().take(max_results).collect())
    }
}
```

**Setup**: `docker run -p 8080:8080 searxng/searxng`

### Premium Option: Exa API

For best quality with semantic search:

```rust
pub struct ExaBackend {
    client: reqwest::Client,
    api_key: String,
    mcp_endpoint: &'static str, // "https://mcp.exa.ai/mcp"
}

impl SearchBackend for ExaBackend {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
        // Uses MCP JSON-RPC format
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "web_search_exa",
                "arguments": {
                    "query": query,
                    "numResults": max_results,
                    "type": "auto"
                }
            }
        });

        let response = self.client
            .post(self.mcp_endpoint)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await?;

        self.parse_mcp_response(response).await
    }
}
```

### Tool Interface

```rust
#[derive(Deserialize, JsonSchema)]
pub struct WebSearchParams {
    /// Search query
    pub query: String,

    /// Maximum number of results (default: 8)
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// Search backend override (default: from config)
    pub backend: Option<SearchBackendType>,

    /// Restrict to specific domains
    pub allowed_domains: Option<Vec<String>>,

    /// Exclude specific domains
    pub blocked_domains: Option<Vec<String>>,

    /// Region for localized results (e.g., "us-en", "uk-en")
    pub region: Option<String>,

    /// Time limit (day, week, month, year)
    pub time_limit: Option<TimeLimit>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchBackendType {
    DuckDuckGo,
    SearXNG,
    Brave,
    Exa,
    Tavily,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TimeLimit {
    Day,
    Week,
    Month,
    Year,
}
```

### Search Result Type

```rust
#[derive(Serialize, JsonSchema)]
pub struct SearchResult {
    /// Result title
    pub title: String,

    /// Result URL
    pub url: String,

    /// Snippet/description
    pub snippet: String,

    /// Source domain
    pub domain: String,

    /// Publication date (if available)
    pub published: Option<String>,

    /// Relevance score (0.0-1.0, if available)
    pub score: Option<f32>,
}
```

---

## Part 4: Configuration

### Config Schema

```toml
[tools.web_fetch]
enabled = true
cache_ttl_seconds = 900           # 15 minutes
max_content_length = 100_000      # characters
default_timeout = 30              # seconds
user_agent = "Crucible/0.1 (AI Assistant; +https://github.com/user/crucible)"

# Browser rendering (requires chromiumoxide feature)
[tools.web_fetch.browser]
enabled = true
chromium_path = ""                # Auto-detect if empty
headless = true

# ReaderLM integration (requires llama-cpp feature)
[tools.web_fetch.reader_model]
enabled = false
model_path = "~/.crucible/models/readerlm-v2-q4_k_m.gguf"
context_size = 32768
gpu_layers = 0                    # 0 = CPU only

# Jina API fallback
[tools.web_fetch.jina]
enabled = true
api_key = ""                      # Optional, uses free tier if empty

[tools.web_search]
enabled = true
default_backend = "duckduckgo"    # duckduckgo, searxng, brave, exa, tavily
max_results = 8

[tools.web_search.duckduckgo]
# No configuration needed

[tools.web_search.searxng]
instance_url = "http://localhost:8080"

[tools.web_search.brave]
api_key = ""

[tools.web_search.exa]
api_key = ""

[tools.web_search.tavily]
api_key = ""
```

---

## Part 5: Headless Browser Details

### Recommended: chromiumoxide

| Attribute | Value |
|-----------|-------|
| **Crate** | [chromiumoxide](https://github.com/mattsse/chromiumoxide) |
| **Version** | 0.8.0 (Nov 2025) |
| **Async** | tokio / async-std |
| **License** | MIT / Apache-2.0 |
| **Stars** | 1.1k |
| **Dependents** | 735 |

### Feature Comparison

| Feature | chromiumoxide | headless_chrome | fantoccini |
|---------|---------------|-----------------|------------|
| **Async** | ✅ tokio | ❌ sync | ✅ tokio |
| **CDP Access** | Full | Partial | WebDriver only |
| **Screenshots** | ✅ | ✅ | ✅ |
| **PDF Export** | ✅ | ✅ | ❌ |
| **Network Intercept** | ✅ | ✅ | ❌ |
| **Auto-download** | ✅ | ✅ | ❌ |
| **Maintenance** | Active | Moderate | Active |

### Usage Example

```rust
use chromiumoxide::{Browser, BrowserConfig, Page};

pub struct BrowserPool {
    browser: Browser,
    config: BrowserConfig,
}

impl BrowserPool {
    pub async fn new() -> Result<Self> {
        let config = BrowserConfig::builder()
            .with_head()  // or .headless() for headless
            .viewport(Some(Viewport { width: 1920, height: 1080 }))
            .build()?;

        let (browser, mut handler) = Browser::launch(config).await?;

        // Spawn handler task
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                // Handle browser events
            }
        });

        Ok(Self { browser, config })
    }

    pub async fn fetch_rendered(&self, url: &str) -> Result<String> {
        let page = self.browser.new_page(url).await?;

        // Wait for page load
        page.wait_for_navigation().await?;

        // Optional: wait for specific element
        // page.wait_for_selector(".content").await?;

        // Get rendered HTML
        let html = page.content().await?;

        // Cleanup
        page.close().await?;

        Ok(html)
    }
}
```

---

## Part 6: Security Considerations

### URL Validation

```rust
pub fn validate_url(url: &str) -> Result<Url, FetchError> {
    let parsed = Url::parse(url).map_err(|_| FetchError::InvalidUrl(url.to_string()))?;

    // Only allow HTTP(S)
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(FetchError::InvalidUrl("Only http/https allowed".into()));
    }

    // Block local addresses
    if let Some(host) = parsed.host_str() {
        if is_local_address(host) {
            return Err(FetchError::BlockedDomain(host.to_string()));
        }
    }

    // Length limit
    if url.len() > 2048 {
        return Err(FetchError::UrlTooLong);
    }

    Ok(parsed)
}

fn is_local_address(host: &str) -> bool {
    host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.starts_with("192.168.")
        || host.starts_with("10.")
        || host.ends_with(".local")
}
```

### Content Limits

- **Max content size**: 5 MB (raw), 100K characters (processed)
- **Max URL length**: 2048 characters
- **Request timeout**: 30s default, 120s max
- **Cache TTL**: 15 minutes

### User-Agent

Use an honest, identifying user agent:

```
Crucible/0.1 (AI Assistant; +https://github.com/user/crucible)
```

This follows best practices and avoids anti-bot systems that block browser-spoofing agents (see [OpenCode issue #2228](https://github.com/sst/opencode/issues/2228)).

---

## Part 7: Implementation Phases

### Phase 1: Core Fetch (MVP)
- [ ] `reqwest` + `htmd` for basic HTML→MD
- [ ] 15-minute caching
- [ ] URL validation and security
- [ ] MCP tool registration

### Phase 2: Browser Rendering
- [ ] `chromiumoxide` integration (feature-gated)
- [ ] JS-rendered page support
- [ ] Automatic fallback from Tier 1

### Phase 3: ReaderLM Integration
- [ ] GGUF model loading via `llama-cpp-2`
- [ ] Configurable model path
- [ ] GPU/CPU selection

### Phase 4: Web Search
- [ ] DuckDuckGo lite scraping (default)
- [ ] SearXNG backend
- [ ] API backends (Exa, Brave, Tavily)

### Phase 5: Advanced Features
- [ ] Jina API fallback
- [ ] Content quality heuristics
- [ ] Screenshot capture
- [ ] PDF extraction

---

## Sources

### Agent Implementations
- [OpenCode - sst/opencode](https://github.com/sst/opencode) — TurndownService, Exa MCP
- [Claude Code Web Tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-fetch-tool) — Two-agent pattern, Brave Search
- [Exa MCP Server](https://github.com/exa-labs/exa-mcp-server) — Semantic search API

### Libraries
- [Jina Reader](https://jina.ai/reader/) — r.jina.ai API, ReaderLM-v2 model
- [Crawl4AI](https://github.com/unclecode/crawl4ai) — Content filtering patterns
- [chromiumoxide](https://github.com/mattsse/chromiumoxide) — Rust CDP API
- [htmd](https://crates.io/crates/htmd) — Turndown.js port for Rust

### Models
- [jinaai/ReaderLM-v2](https://huggingface.co/jinaai/ReaderLM-v2) — Original model
- [rbehzadan/ReaderLM-v2.gguf](https://huggingface.co/rbehzadan/ReaderLM-v2.gguf) — GGUF quantizations

### Search Backends
- [SearXNG](https://docs.searxng.org/) — Self-hosted metasearch
- [DuckDuckGo Lite](https://lite.duckduckgo.com/) — Scraping target
- [Exa API](https://docs.exa.ai/) — Semantic search
- [Brave Search API](https://brave.com/search/api/) — Web search
