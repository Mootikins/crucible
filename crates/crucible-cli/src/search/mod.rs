//! Text search backends

pub mod factory;
pub mod regex_backend;
pub mod ripgrep_backend;
pub mod wikilink;

use anyhow::Result;
use crucible_core::traits::TextSearchMatch;
use std::path::PathBuf;

pub use factory::{create_text_searcher, detect_backend, SearchBackend};
pub use regex_backend::RegexSearcher;
pub use ripgrep_backend::RipgrepSearcher;
pub use wikilink::find_backlinks;

/// Concrete text search backend (enum dispatch, no dynamic dispatch)
pub enum TextSearchBackend {
    Regex(RegexSearcher),
    Ripgrep(RipgrepSearcher),
}

impl TextSearchBackend {
    pub async fn search(&self, pattern: &str, paths: &[PathBuf]) -> Result<Vec<TextSearchMatch>> {
        match self {
            Self::Regex(s) => s.search(pattern, paths).await,
            Self::Ripgrep(s) => s.search(pattern, paths).await,
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Regex(s) => s.backend_name(),
            Self::Ripgrep(s) => s.backend_name(),
        }
    }
}
