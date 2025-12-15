//! Interactive utilities for CLI
//!
//! NOTE: Fuzzy matching stubbed - nucleo removed during event architecture cleanup.
//! Uses simple substring matching as fallback.

use anyhow::Result;
use crucible_core::database::SearchResult;
use std::io::{self, Write};

/// Compatibility wrapper for search results with display information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResultWithScore {
    pub id: String,
    pub title: String,
    pub content: String,
    pub score: f64,
}

impl From<SearchResult> for SearchResultWithScore {
    fn from(result: SearchResult) -> Self {
        let doc_id = result.document_id.0; // Move once
        Self {
            id: doc_id.clone(),
            title: doc_id, // Use note ID as title for now
            content: result.snippet.unwrap_or_default(),
            score: result.score,
        }
    }
}

/// Interactive picker (fuzzy matching stubbed pending event architecture)
pub struct FuzzyPicker;

impl FuzzyPicker {
    pub fn new() -> Self {
        Self
    }

    /// Pick from search results interactively
    pub fn pick_result(&mut self, results: &[SearchResultWithScore]) -> Result<Option<usize>> {
        if results.is_empty() {
            return Ok(None);
        }

        self.print_results(results)?;
        self.get_selection(results.len())
    }

    fn print_results(&self, results: &[SearchResultWithScore]) -> Result<()> {
        println!("\n{} results found:\n", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!("{:2}. {} ", idx + 1, result.title);
            println!("    {}", result.id);
            let preview: String = result.content.lines().take(1).collect::<Vec<_>>().join(" ");
            let truncated = if preview.len() > 80 {
                format!("{}...", &preview[..80])
            } else {
                preview
            };
            println!("    {}\n", truncated);
        }
        Ok(())
    }

    fn get_selection(&self, max: usize) -> Result<Option<usize>> {
        print!("Select (1-{}, or 'q' to quit): ", max);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() || input == "q" {
            return Ok(None);
        }

        match input.parse::<usize>() {
            Ok(n) if n >= 1 && n <= max => Ok(Some(n - 1)),
            _ => {
                println!("Invalid selection");
                Ok(None)
            }
        }
    }

    /// Filter items by query using simple substring matching
    /// (nucleo fuzzy matching removed - will be reimplemented with event architecture)
    pub fn filter_items(&mut self, items: &[String], query: &str) -> Vec<(usize, u32)> {
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        for (idx, item) in items.iter().enumerate() {
            let item_lower = item.to_lowercase();
            if query.is_empty() || item_lower.contains(&query_lower) {
                // Score based on position (earlier = better) and length match
                let score = if query.is_empty() {
                    100
                } else if item_lower == query_lower {
                    1000 // Exact match
                } else if item_lower.starts_with(&query_lower) {
                    500 // Prefix match
                } else {
                    100 // Substring match
                };
                matches.push((idx, score));
            }
        }

        // Sort by score descending
        matches.sort_by(|a, b| b.1.cmp(&a.1));
        matches
    }
}

impl Default for FuzzyPicker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_picker_initialization() {
        let _picker = FuzzyPicker::new();
        assert!(true);
    }

    #[test]
    fn test_filter_items_exact_match() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "hello.md".to_string(),
            "world.md".to_string(),
            "test.md".to_string(),
        ];

        let results = picker.filter_items(&items, "hello.md");
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 0); // Index of "hello.md"
    }

    #[test]
    fn test_filter_items_substring_match() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "my-note.md".to_string(),
            "your-note.md".to_string(),
            "other-file.md".to_string(),
        ];

        let results = picker.filter_items(&items, "note");
        assert_eq!(results.len(), 2); // Both notes match
    }

    #[test]
    fn test_filter_items_empty_query() {
        let mut picker = FuzzyPicker::new();
        let items = vec!["file1.md".to_string(), "file2.md".to_string()];

        let results = picker.filter_items(&items, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_filter_items_empty_list() {
        let mut picker = FuzzyPicker::new();
        let items: Vec<String> = vec![];

        let results = picker.filter_items(&items, "test");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_filter_items_case_insensitive() {
        let mut picker = FuzzyPicker::new();
        let items = vec!["HelloWorld.md".to_string(), "goodbye.md".to_string()];

        let results = picker.filter_items(&items, "helloworld");
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 0);
    }

    #[test]
    fn test_filter_items_no_match() {
        let mut picker = FuzzyPicker::new();
        let items = vec!["foo.md".to_string(), "bar.md".to_string()];

        let results = picker.filter_items(&items, "xyz");
        assert_eq!(results.len(), 0);
    }
}
