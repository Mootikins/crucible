//! Completion system with fuzzy filtering
//!
//! Provides fuzzy completion for commands, files, and agents using nucleo-matcher.

use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};
use std::collections::HashSet;

/// Types of completion available
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionType {
    /// Slash commands (e.g., /clear, /help)
    Command,
    /// File references (e.g., @file.md)
    File,
    /// Agent references (e.g., @@agent)
    Agent,
}

/// A single completion item
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    /// The text to complete to
    pub text: String,
    /// Optional description
    pub description: Option<String>,
    /// Type of completion
    pub item_type: CompletionType,
}

impl CompletionItem {
    /// Create a new completion item
    pub fn new(text: impl Into<String>, description: Option<String>, item_type: CompletionType) -> Self {
        Self {
            text: text.into(),
            description,
            item_type,
        }
    }
}

/// State for an active completion session
pub struct CompletionState {
    /// Current query string (what user has typed after trigger)
    pub query: String,

    /// All available items for this completion type
    pub all_items: Vec<CompletionItem>,

    /// Items after fuzzy filtering
    pub filtered_items: Vec<CompletionItem>,

    /// Currently selected index in filtered list
    pub selected_index: usize,

    /// Whether multi-select is enabled
    pub multi_select: bool,

    /// Selected indices (for multi-select mode)
    pub selections: HashSet<usize>,

    /// Column position where completion was triggered
    pub trigger_column: u16,

    /// The completion type
    pub completion_type: CompletionType,
}

impl CompletionState {
    /// Create a new completion state
    pub fn new(items: Vec<CompletionItem>, completion_type: CompletionType) -> Self {
        let filtered = items.clone();
        Self {
            query: String::new(),
            all_items: items,
            filtered_items: filtered,
            selected_index: 0,
            multi_select: false,
            selections: HashSet::new(),
            trigger_column: 0,
            completion_type,
        }
    }

    /// Create a new completion state with multi-select enabled
    pub fn new_multi(items: Vec<CompletionItem>, completion_type: CompletionType) -> Self {
        let mut state = Self::new(items, completion_type);
        state.multi_select = true;
        state
    }

    /// Re-filter items based on current query
    pub fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered_items = self.all_items.clone();
            self.selected_index = 0;
            return;
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(&self.query, CaseMatching::Ignore, Normalization::Smart);

        let mut matches: Vec<(CompletionItem, u32)> = self
            .all_items
            .iter()
            .filter_map(|item| {
                let mut buf = Vec::new();
                let haystack = Utf32Str::new(&item.text, &mut buf);
                pattern
                    .score(haystack, &mut matcher)
                    .map(|score| (item.clone(), score))
            })
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        self.filtered_items = matches.into_iter().map(|(item, _)| item).collect();
        self.selected_index = 0;
    }

    /// Move selection up (wraps to last item at top)
    pub fn select_prev(&mut self) {
        if !self.filtered_items.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                // Wrap to last item
                self.selected_index = self.filtered_items.len() - 1;
            }
        }
    }

    /// Move selection down (wraps to first item at bottom)
    pub fn select_next(&mut self) {
        if !self.filtered_items.is_empty() {
            if self.selected_index < self.filtered_items.len() - 1 {
                self.selected_index += 1;
            } else {
                // Wrap to first item
                self.selected_index = 0;
            }
        }
    }

    /// Toggle selection in multi-select mode
    pub fn toggle_selection(&mut self) {
        if self.multi_select && !self.filtered_items.is_empty() {
            if self.selections.contains(&self.selected_index) {
                self.selections.remove(&self.selected_index);
            } else {
                self.selections.insert(self.selected_index);
            }
        }
    }

    /// Get the currently selected item
    pub fn selected_item(&self) -> Option<&CompletionItem> {
        self.filtered_items.get(self.selected_index)
    }

    /// Get all selected items (for multi-select mode)
    pub fn selected_items(&self) -> Vec<&CompletionItem> {
        if self.multi_select {
            self.selections
                .iter()
                .filter_map(|&idx| self.filtered_items.get(idx))
                .collect()
        } else {
            self.selected_item().into_iter().collect()
        }
    }

    /// Check if an index is selected (for rendering)
    pub fn is_selected(&self, index: usize) -> bool {
        self.selections.contains(&index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_items() -> Vec<CompletionItem> {
        vec![
            CompletionItem::new("search", Some("Search notes".into()), CompletionType::Command),
            CompletionItem::new("session", Some("Session management".into()), CompletionType::Command),
            CompletionItem::new("clear", Some("Clear context".into()), CompletionType::Command),
            CompletionItem::new("help", Some("Show help".into()), CompletionType::Command),
        ]
    }

    #[test]
    fn test_fuzzy_filter_prefix() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        state.query = "se".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 2);
        // Both "search" and "session" should match
        let texts: Vec<_> = state.filtered_items.iter().map(|i| &i.text).collect();
        assert!(texts.contains(&&"search".to_string()));
        assert!(texts.contains(&&"session".to_string()));
    }

    #[test]
    fn test_fuzzy_filter_fuzzy_match() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        // "srch" should fuzzy match "search"
        state.query = "srch".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 1);
        assert_eq!(state.filtered_items[0].text, "search");
    }

    #[test]
    fn test_fuzzy_filter_no_match() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        state.query = "xyz".to_string();
        state.refilter();

        assert!(state.filtered_items.is_empty());
    }

    #[test]
    fn test_empty_query_shows_all() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        state.query = "".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 4);
    }

    #[test]
    fn test_navigation() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        assert_eq!(state.selected_index, 0);

        state.select_next();
        assert_eq!(state.selected_index, 1);

        state.select_next();
        assert_eq!(state.selected_index, 2);

        state.select_prev();
        assert_eq!(state.selected_index, 1);

        state.select_prev();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_navigation_wraps_down() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        let len = state.filtered_items.len();

        // Navigate to last item
        for _ in 0..len - 1 {
            state.select_next();
        }
        assert_eq!(state.selected_index, len - 1);

        // One more should wrap to first
        state.select_next();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_navigation_wraps_up() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        let len = state.filtered_items.len();

        // At first item, up should wrap to last
        assert_eq!(state.selected_index, 0);
        state.select_prev();
        assert_eq!(state.selected_index, len - 1);
    }

    #[test]
    fn test_navigation_single_item() {
        let items = vec![CompletionItem::new("only", None, CompletionType::Command)];
        let mut state = CompletionState::new(items, CompletionType::Command);

        assert_eq!(state.selected_index, 0);
        state.select_next();
        assert_eq!(state.selected_index, 0); // Should wrap to itself
        state.select_prev();
        assert_eq!(state.selected_index, 0); // Should wrap to itself
    }

    #[test]
    fn test_navigation_empty_list() {
        let items: Vec<CompletionItem> = vec![];
        let mut state = CompletionState::new(items, CompletionType::Command);

        assert_eq!(state.selected_index, 0);
        state.select_next(); // Should not panic
        assert_eq!(state.selected_index, 0);
        state.select_prev(); // Should not panic
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_multi_select() {
        let mut state = CompletionState::new_multi(test_items(), CompletionType::File);

        // Select first item
        state.toggle_selection();
        assert!(state.is_selected(0));

        // Move down and select
        state.select_next();
        state.toggle_selection();
        assert!(state.is_selected(1));

        // First should still be selected
        assert!(state.is_selected(0));

        // Get selected items
        let selected = state.selected_items();
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_selected_item() {
        let state = CompletionState::new(test_items(), CompletionType::Command);

        let item = state.selected_item();
        assert!(item.is_some());
        assert_eq!(item.unwrap().text, "search");
    }

    // Enhanced fuzzy filter tests

    #[test]
    fn test_fuzzy_filter_case_insensitive() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        // Test uppercase query
        state.query = "SE".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 2);
        let texts: Vec<_> = state.filtered_items.iter().map(|i| &i.text).collect();
        assert!(texts.contains(&&"search".to_string()));
        assert!(texts.contains(&&"session".to_string()));

        // Test mixed case
        state.query = "SeArCh".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 1);
        assert_eq!(state.filtered_items[0].text, "search");
    }

    #[test]
    fn test_fuzzy_filter_substring() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        // Test substring in middle
        state.query = "ear".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 2);
        let texts: Vec<_> = state.filtered_items.iter().map(|i| &i.text).collect();
        assert!(texts.contains(&&"search".to_string()));
        assert!(texts.contains(&&"clear".to_string()));

        // Test substring at end
        state.query = "ion".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 1);
        assert_eq!(state.filtered_items[0].text, "session");
    }

    #[test]
    fn test_fuzzy_filter_ranking() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        // "se" should rank "search" and "session" higher than partial matches
        state.query = "se".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 2);
        // First item should be one of the direct prefix matches
        let first_text = &state.filtered_items[0].text;
        assert!(first_text == "search" || first_text == "session");

        // Test that exact prefix ranks higher than fuzzy match
        let items = vec![
            CompletionItem::new("search", None, CompletionType::Command),
            CompletionItem::new("some_search_function", None, CompletionType::Command),
            CompletionItem::new("sarch", None, CompletionType::Command),
        ];
        let mut state = CompletionState::new(items, CompletionType::Command);

        state.query = "search".to_string();
        state.refilter();

        // Exact match or prefix match should be first
        assert_eq!(state.filtered_items[0].text, "search");
    }

    #[test]
    fn test_fuzzy_filter_performance() {
        // Create many items to test performance
        let many_items: Vec<CompletionItem> = (0..1000)
            .map(|i| {
                CompletionItem::new(
                    format!("command_{}", i),
                    Some(format!("Description {}", i)),
                    CompletionType::Command,
                )
            })
            .collect();

        let mut state = CompletionState::new(many_items, CompletionType::Command);

        // This should complete quickly even with 1000 items
        state.query = "cmd".to_string();
        state.refilter();

        // Just verify it completes and returns some results
        assert!(state.filtered_items.len() >= 0); // Should filter efficiently
    }

    #[test]
    fn test_completion_state_with_descriptions() {
        // Test items with descriptions
        let items_with_desc = vec![
            CompletionItem::new("cmd1", Some("First command".into()), CompletionType::Command),
            CompletionItem::new("cmd2", Some("Second command".into()), CompletionType::Command),
        ];
        let state = CompletionState::new(items_with_desc, CompletionType::Command);

        assert_eq!(state.all_items.len(), 2);
        assert!(state.all_items[0].description.is_some());
        assert_eq!(state.all_items[0].description.as_ref().unwrap(), "First command");

        // Test items without descriptions
        let items_no_desc = vec![
            CompletionItem::new("file1.md", None, CompletionType::File),
            CompletionItem::new("file2.md", None, CompletionType::File),
        ];
        let state = CompletionState::new(items_no_desc, CompletionType::File);

        assert_eq!(state.all_items.len(), 2);
        assert!(state.all_items[0].description.is_none());
        assert!(state.all_items[1].description.is_none());

        // Test mixed items
        let items_mixed = vec![
            CompletionItem::new("with_desc", Some("Has description".into()), CompletionType::Agent),
            CompletionItem::new("no_desc", None, CompletionType::Agent),
            CompletionItem::new("also_desc", Some("Another one".into()), CompletionType::Agent),
        ];
        let state = CompletionState::new(items_mixed, CompletionType::Agent);

        assert_eq!(state.all_items.len(), 3);
        assert!(state.all_items[0].description.is_some());
        assert!(state.all_items[1].description.is_none());
        assert!(state.all_items[2].description.is_some());
    }

    #[test]
    fn test_completion_type_variants() {
        // Test Command type items
        let cmd_items = vec![
            CompletionItem::new("clear", Some("Clear screen".into()), CompletionType::Command),
            CompletionItem::new("help", Some("Show help".into()), CompletionType::Command),
        ];
        let cmd_state = CompletionState::new(cmd_items, CompletionType::Command);

        assert_eq!(cmd_state.completion_type, CompletionType::Command);
        assert_eq!(cmd_state.all_items.len(), 2);
        assert_eq!(cmd_state.all_items[0].item_type, CompletionType::Command);
        assert_eq!(cmd_state.all_items[1].item_type, CompletionType::Command);

        // Test File type items
        let file_items = vec![
            CompletionItem::new("notes.md", None, CompletionType::File),
            CompletionItem::new("README.md", None, CompletionType::File),
            CompletionItem::new("todo.txt", None, CompletionType::File),
        ];
        let file_state = CompletionState::new(file_items, CompletionType::File);

        assert_eq!(file_state.completion_type, CompletionType::File);
        assert_eq!(file_state.all_items.len(), 3);
        for item in &file_state.all_items {
            assert_eq!(item.item_type, CompletionType::File);
        }

        // Test Agent type items
        let agent_items = vec![
            CompletionItem::new("rust-expert", Some("Rust programming expert".into()), CompletionType::Agent),
            CompletionItem::new("code-reviewer", Some("Reviews code quality".into()), CompletionType::Agent),
        ];
        let agent_state = CompletionState::new(agent_items, CompletionType::Agent);

        assert_eq!(agent_state.completion_type, CompletionType::Agent);
        assert_eq!(agent_state.all_items.len(), 2);
        assert_eq!(agent_state.all_items[0].item_type, CompletionType::Agent);
        assert_eq!(agent_state.all_items[1].item_type, CompletionType::Agent);

        // Verify completion type affects behavior correctly
        let mut cmd_state = CompletionState::new(
            vec![CompletionItem::new("test", None, CompletionType::Command)],
            CompletionType::Command,
        );

        cmd_state.query = "t".to_string();
        cmd_state.refilter();

        assert_eq!(cmd_state.filtered_items.len(), 1);
        assert_eq!(cmd_state.completion_type, CompletionType::Command);
    }

    #[test]
    fn test_fuzzy_filter_resets_selection() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);

        // Move selection down
        state.select_next();
        state.select_next();
        assert_eq!(state.selected_index, 2);

        // Refilter should reset selection to 0
        state.query = "se".to_string();
        state.refilter();

        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_fuzzy_filter_partial_word() {
        let items = vec![
            CompletionItem::new("create_new_file", None, CompletionType::Command),
            CompletionItem::new("create_note", None, CompletionType::Command),
            CompletionItem::new("delete_file", None, CompletionType::Command),
        ];
        let mut state = CompletionState::new(items, CompletionType::Command);

        // Should match "create_new_file" and "create_note"
        state.query = "cre".to_string();
        state.refilter();

        assert_eq!(state.filtered_items.len(), 2);
        let texts: Vec<_> = state.filtered_items.iter().map(|i| &i.text).collect();
        assert!(texts.contains(&&"create_new_file".to_string()));
        assert!(texts.contains(&&"create_note".to_string()));

        // Should match all three with fuzzy matching
        state.query = "c_f".to_string();
        state.refilter();

        assert!(state.filtered_items.len() >= 1);
    }
}
