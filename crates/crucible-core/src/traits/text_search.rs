//! Text search abstraction for full-text search across files

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

/// A match from text search
#[derive(Debug, Clone, PartialEq)]
pub struct TextSearchMatch {
    pub path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

/// Abstract interface for text search
#[async_trait]
pub trait TextSearcher: Send + Sync {
    /// Search for pattern in files under given paths
    async fn search(&self, pattern: &str, paths: &[PathBuf]) -> Result<Vec<TextSearchMatch>>;

    /// Name of the backend (for status/logging)
    fn backend_name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSearcher {
        results: Vec<TextSearchMatch>,
    }

    #[async_trait]
    impl TextSearcher for MockSearcher {
        async fn search(&self, _pattern: &str, _paths: &[PathBuf]) -> Result<Vec<TextSearchMatch>> {
            Ok(self.results.clone())
        }
        fn backend_name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_mock_searcher_returns_results() {
        let searcher = MockSearcher {
            results: vec![TextSearchMatch {
                path: PathBuf::from("test.md"),
                line_number: 1,
                line_content: "hello world".to_string(),
                match_start: 0,
                match_end: 5,
            }],
        };
        let results = searcher.search("hello", &[]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_content, "hello world");
    }
}
