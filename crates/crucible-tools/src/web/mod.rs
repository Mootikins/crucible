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

mod config;

pub use config::WebTools;

// Modules to be added:
// mod cache;
// mod fetch;
// mod search;
