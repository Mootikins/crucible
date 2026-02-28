//! Factory for creating text search backends

use crate::search::{RegexSearcher, RipgrepSearcher, TextSearchBackend};
use tracing::info;

/// Detected text search backend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBackend {
    Ripgrep,
    Regex,
}

/// Detect best available search backend
pub async fn detect_backend() -> SearchBackend {
    if RipgrepSearcher::is_available().await {
        info!("Text search: using ripgrep");
        SearchBackend::Ripgrep
    } else {
        info!("Text search: ripgrep not found, using regex fallback");
        SearchBackend::Regex
    }
}

/// Create a text searcher based on available backends
pub async fn create_text_searcher() -> TextSearchBackend {
    match detect_backend().await {
        SearchBackend::Ripgrep => TextSearchBackend::Ripgrep(RipgrepSearcher::new()),
        SearchBackend::Regex => TextSearchBackend::Regex(RegexSearcher::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_text_searcher_returns_valid_backend() {
        let searcher = create_text_searcher().await;
        let name = searcher.backend_name();
        assert!(name == "ripgrep" || name == "regex");
    }
}
