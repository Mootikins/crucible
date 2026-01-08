//! Web tools for fetching and searching the web
//!
//! Provides two MCP tools:
//! - `web_fetch` - Fetch URL, convert HTML to markdown, optionally summarize
//! - `web_search` - Search the web via configurable provider (SearXNG)
//!
//! ## Configuration
//!
//! Web tools are disabled by default. Enable in config:
//!
//! ```toml
//! [web_tools]
//! enabled = true
//! ```

mod cache;
mod config;
mod fetch;
mod search;

pub use cache::FetchCache;
pub use config::{WebTools, WebToolsError};
pub use fetch::{create_client, fetch_and_convert, FetchError};
pub use search::{SearchError, SearchResult, SearxngProvider, WebSearchProvider};
