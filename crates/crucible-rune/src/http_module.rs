//! HTTP module for Rune scripts.
//!
//! Re-exports the built-in `rune_modules::http` module which provides HTTP
//! client functionality for making network requests from Rune scripts.
//!
//! # Example
//!
//! ```rune
//! use ::http;
//!
//! pub async fn main() {
//!     // Create a client
//!     let client = http::Client::new();
//!
//!     // GET request
//!     let response = client.get("https://api.example.com/data").send().await?;
//!     let body = response.text().await?;
//!     println!("{}", body);
//!
//!     // POST request with body
//!     let response = client.post("https://api.example.com/users")
//!         .body("name=Alice")
//!         .send()
//!         .await?;
//! }
//! ```
//!
//! # Available Types
//!
//! - `http::Client` - HTTP client for making requests
//! - `http::RequestBuilder` - Configure request properties
//! - `http::Response` - Handle HTTP responses
//! - `http::StatusCode` - HTTP status code
//! - `http::Error` - Error type for HTTP operations
//!
//! # Note
//!
//! This module uses the rune-modules HTTP implementation which is based on reqwest.

use rune::{ContextError, Module};

/// Create the HTTP module for Rune.
///
/// The `stdio` parameter controls whether stdio operations are enabled.
/// Pass `true` for normal usage, `false` for sandboxed environments.
pub fn http_module(stdio: bool) -> Result<Module, ContextError> {
    rune_modules::http::module(stdio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_module_creation() {
        let module = http_module(true);
        assert!(module.is_ok(), "Should create http module");
    }

    /// Test http module from Rune script (compilation only - no network)
    #[test]
    fn test_http_from_rune() {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources};
        use std::sync::Arc;

        let mut context = Context::with_default_modules().unwrap();
        context.install(http_module(true).unwrap()).unwrap();
        let _runtime = Arc::new(context.runtime().unwrap());

        let script = r#"
            pub async fn make_request() {
                // Just test that the API compiles
                let client = http::Client::new();
                let request = client.get("https://example.com");
                // Don't actually send - would need network
            }
        "#;

        let mut sources = Sources::new();
        sources
            .insert(Source::new("test", script).unwrap())
            .unwrap();

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources).unwrap();
        }

        assert!(result.is_ok(), "Should compile script with http module");
    }
}
