use anyhow::Result;
use crucible_core::database::SearchResult;
use nucleo_matcher::{pattern::{Pattern, CaseMatching}, Matcher, Config};
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
            title: doc_id, // Use document ID as title for now
            content: result.snippet.unwrap_or_default(),
            score: result.score,
        }
    }
}

/// Interactive fuzzy picker using nucleo
pub struct FuzzyPicker {
    matcher: Matcher,
}

impl FuzzyPicker {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Pick from search results interactively
    pub fn pick_result(&mut self, results: &[SearchResultWithScore]) -> Result<Option<usize>> {
        if results.is_empty() {
            return Ok(None);
        }

        // For now, implement simple numbered selection
        // TODO: Full interactive picker with nucleo in future iteration
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

    /// Filter items by query using nucleo matcher
    pub fn filter_items(&mut self, items: &[String], query: &str) -> Vec<(usize, u32)> {
        use nucleo_matcher::pattern::Normalization;
        use nucleo_matcher::Utf32Str;

        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut matches = Vec::new();

        for (idx, item) in items.iter().enumerate() {
            let mut buf = Vec::new();
            let haystack = Utf32Str::new(item, &mut buf);
            if let Some(score) = pattern.score(haystack, &mut self.matcher) {
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
        // Just verify it doesn't panic
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

        let results = picker.filter_items(&items, "hello");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 0); // Index of "hello.md"
    }

    #[test]
    fn test_filter_items_fuzzy_match() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "my-document.md".to_string(),
            "your-document.md".to_string(),
            "other-file.md".to_string(),
        ];

        let results = picker.filter_items(&items, "mydoc");

        assert!(results.len() > 0);
        assert_eq!(results[0].0, 0); // Should match "my-document.md"
    }

    #[test]
    fn test_filter_items_score_ordering() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "test.md".to_string(),
            "testing.md".to_string(),
            "test-file.md".to_string(),
        ];

        let results = picker.filter_items(&items, "test");

        // All items should match
        assert!(results.len() >= 2);
        // Results should be sorted by score (descending)
        if results.len() > 1 {
            assert!(results[0].1 >= results[1].1);
        }
    }

    #[test]
    fn test_filter_items_empty_query() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "file1.md".to_string(),
            "file2.md".to_string(),
        ];

        let results = picker.filter_items(&items, "");

        // Empty query should match all items
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
        let items = vec![
            "HelloWorld.md".to_string(),
            "goodbye.md".to_string(),
        ];

        let results = picker.filter_items(&items, "helloworld");

        assert!(results.len() > 0);
        assert_eq!(results[0].0, 0); // Should match "HelloWorld.md"
    }

    #[test]
    fn test_filter_items_no_match() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "foo.md".to_string(),
            "bar.md".to_string(),
        ];

        let results = picker.filter_items(&items, "xyz");

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_filter_items_special_characters() {
        let mut picker = FuzzyPicker::new();
        let items = vec![
            "file-with-dashes.md".to_string(),
            "file_with_underscores.md".to_string(),
        ];

        let results = picker.filter_items(&items, "dashes");

        assert!(results.len() > 0);
        assert_eq!(results[0].0, 0);
    }
}
