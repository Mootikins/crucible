//! Text search types

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
