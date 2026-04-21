//! Simple TOML include mechanism
//!
//! This module provides ways to include external content into configuration:
//!
//! ## 1. File References: `{file:path}`
//!
//! Use `{file:path}` anywhere in your config to pull in external content:
//!
//! ```toml
//! # Include a whole section from a TOML file
//! gateway = "{file:mcps.toml}"
//!
//! # Include just a secret value
//! [embedding]
//! provider = "openai"
//! api_key = "{file:~/.secrets/openai.key}"
//!
//! # Works in arrays too
//! extra_paths = ["{file:paths.toml}"]
//! ```
//!
//! - If the file is `.toml`, it's parsed and merged as structured data
//! - Otherwise, the file content is used as a string value (trimmed)
//! - Paths can be relative, absolute, or use `~` for home directory
//!
//! ## 2. Environment Variables: `{env:VAR}`
//!
//! Use `{env:VAR}` to read values from environment variables:
//!
//! ```toml
//! [embedding]
//! provider = "openai"
//! api_key = "{env:OPENAI_API_KEY}"
//!
//! [providers.anthropic]
//! api_key = "{env:ANTHROPIC_API_KEY}"
//! ```
//!
//! - In `BestEffort` mode (default), missing env vars log a warning and continue
//! - In `Strict` mode, missing env vars are treated as hard errors
//! - Use this for secrets that shouldn't be in files
//!
//! ## 3. Directory References: `{dir:path}` (config.d style)
//!
//! Use `{dir:path}` to merge all `.toml` files from a directory:
//!
//! ```toml
//! # Include all provider configs from a directory
//! providers = "{dir:~/.config/crucible/providers.d/}"
//! ```
//!
//! Files in the directory are processed in sorted order (alphabetically),
//! allowing predictable override behavior with numeric prefixes:
//! - `00-base.toml` - processed first
//! - `10-cloud.toml` - processed second
//! - `99-override.toml` - processed last, overrides earlier values
//!
//! Non-`.toml` files and hidden files (starting with `.`) are ignored.
//!
//! ## 4. Include Section (legacy)
//!
//! The `[include]` section merges files into specific top-level sections:
//!
//! ```toml
//! [include]
//! gateway = "mcps.toml"
//! ```
//!
//! This merges `mcps.toml` into the `gateway` section.

mod config;
mod error;
mod merge;
mod path;
mod process;
mod reference;

#[cfg(all(test, feature = "toml"))]
mod tests;

pub use config::IncludeConfig;
pub use error::IncludeError;
#[cfg(feature = "toml")]
pub use merge::merge_includes;
#[allow(unused_imports)]
pub use path::resolve_include_path;
#[cfg(feature = "toml")]
pub use process::process_file_references;
#[allow(unused_imports)]
#[cfg(feature = "toml")]
pub use reference::read_include_file;
pub use reference::ResolveMode;
