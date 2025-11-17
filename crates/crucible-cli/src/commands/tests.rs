//! Tests for CLI commands

#[cfg(test)]
mod chat_tests {
    use crate::commands::chat::ChatMode;

    #[test]
    fn test_chat_mode_display_name() {
        assert_eq!(ChatMode::Plan.display_name(), "plan");
        assert_eq!(ChatMode::Act.display_name(), "act");
    }

    #[test]
    fn test_chat_mode_toggle() {
        assert_eq!(ChatMode::Plan.toggle(), ChatMode::Act);
        assert_eq!(ChatMode::Act.toggle(), ChatMode::Plan);

        // Test toggle is reversible
        let mode = ChatMode::Plan;
        assert_eq!(mode.toggle().toggle(), mode);
    }

    #[test]
    fn test_chat_mode_is_read_only() {
        assert!(ChatMode::Plan.is_read_only());
        assert!(!ChatMode::Act.is_read_only());
    }

    #[test]
    fn test_chat_mode_equality() {
        assert_eq!(ChatMode::Plan, ChatMode::Plan);
        assert_eq!(ChatMode::Act, ChatMode::Act);
        assert_ne!(ChatMode::Plan, ChatMode::Act);
    }

    #[test]
    fn test_chat_mode_clone() {
        let mode = ChatMode::Plan;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_chat_mode_debug() {
        // Test that Debug is implemented
        let plan_debug = format!("{:?}", ChatMode::Plan);
        let act_debug = format!("{:?}", ChatMode::Act);

        assert!(plan_debug.contains("Plan"));
        assert!(act_debug.contains("Act"));
    }

    #[test]
    fn test_chat_mode_toggle_multiple_times() {
        let mut mode = ChatMode::Plan;

        // Toggle many times and verify state
        for i in 0..10 {
            mode = mode.toggle();
            if i % 2 == 0 {
                assert_eq!(mode, ChatMode::Act);
            } else {
                assert_eq!(mode, ChatMode::Plan);
            }
        }
    }

    #[test]
    fn test_chat_mode_display_formatting() {
        // Test display formatting for UI
        let plan_display = ChatMode::Plan.display_name();
        let act_display = ChatMode::Act.display_name();

        // Verify lowercase for consistency in UI
        assert_eq!(plan_display, plan_display.to_lowercase());
        assert_eq!(act_display, act_display.to_lowercase());
    }

    #[test]
    fn test_chat_mode_permissions() {
        // Plan mode should be read-only
        assert!(ChatMode::Plan.is_read_only());
        assert_eq!(ChatMode::Plan.display_name(), "plan");

        // Act mode should allow writes
        assert!(!ChatMode::Act.is_read_only());
        assert_eq!(ChatMode::Act.display_name(), "act");
    }
}

#[cfg(test)]
mod process_tests {
    use crate::commands::process::is_markdown_file;
    use std::path::Path;

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file(Path::new("test.md")));
        assert!(is_markdown_file(Path::new("/path/to/note.md")));
        assert!(is_markdown_file(Path::new("README.md")));

        assert!(!is_markdown_file(Path::new("test.txt")));
        assert!(!is_markdown_file(Path::new("test.rs")));
        assert!(!is_markdown_file(Path::new("test")));
        assert!(!is_markdown_file(Path::new("test.MD"))); // Case sensitive
    }

    #[test]
    fn test_is_markdown_file_edge_cases() {
        // Files that end with .md but have other extensions
        assert!(!is_markdown_file(Path::new("file.md.txt")));

        // Hidden files
        assert!(is_markdown_file(Path::new(".hidden.md")));

        // No extension
        assert!(!is_markdown_file(Path::new("README")));
    }

    #[test]
    fn test_is_markdown_file_various_extensions() {
        // Valid markdown extensions
        assert!(is_markdown_file(Path::new("note.md")));
        assert!(is_markdown_file(Path::new("file.md")));

        // Invalid extensions (case sensitive)
        assert!(!is_markdown_file(Path::new("note.MD")));
        assert!(!is_markdown_file(Path::new("note.Md")));
        assert!(!is_markdown_file(Path::new("note.mD")));

        // Other common text formats
        assert!(!is_markdown_file(Path::new("note.txt")));
        assert!(!is_markdown_file(Path::new("note.rst")));
        assert!(!is_markdown_file(Path::new("note.org")));
        assert!(!is_markdown_file(Path::new("note.adoc")));
    }

    #[test]
    fn test_is_markdown_file_with_directories() {
        // Directory-like paths
        assert!(is_markdown_file(Path::new("dir/file.md")));
        assert!(is_markdown_file(Path::new("./file.md")));
        assert!(is_markdown_file(Path::new("../file.md")));
        assert!(is_markdown_file(Path::new("/absolute/path/file.md")));

        // But not directories themselves
        assert!(!is_markdown_file(Path::new("directory")));
        assert!(!is_markdown_file(Path::new("directory/")));
    }

    #[test]
    fn test_is_markdown_file_special_characters() {
        // Files with special characters in name
        assert!(is_markdown_file(Path::new("my-note.md")));
        assert!(is_markdown_file(Path::new("my_note.md")));
        assert!(is_markdown_file(Path::new("my note.md")));
        assert!(is_markdown_file(Path::new("my.note.md")));

        // Unicode characters
        assert!(is_markdown_file(Path::new("日本語.md")));
        assert!(is_markdown_file(Path::new("émoji-📝.md")));
    }

    #[test]
    fn test_is_markdown_file_empty_path() {
        // Empty path should not be markdown
        assert!(!is_markdown_file(Path::new("")));
    }

    #[test]
    fn test_is_markdown_file_just_extension() {
        // Just the extension without a name is not a valid markdown file
        assert!(!is_markdown_file(Path::new(".md")));
    }
}

#[cfg(test)]
mod core_facade_tests {
    use crate::core_facade::SemanticSearchResult;

    #[test]
    fn test_semantic_search_result_creation() {
        let result = SemanticSearchResult {
            doc_id: "test/note.md".to_string(),
            title: "Test Note".to_string(),
            snippet: "This is a test snippet".to_string(),
            similarity: 0.95,
        };

        assert_eq!(result.doc_id, "test/note.md");
        assert_eq!(result.title, "Test Note");
        assert_eq!(result.snippet, "This is a test snippet");
        assert_eq!(result.similarity, 0.95);
    }

    #[test]
    fn test_semantic_search_result_clone() {
        let result = SemanticSearchResult {
            doc_id: "test.md".to_string(),
            title: "Test".to_string(),
            snippet: "Snippet".to_string(),
            similarity: 0.8,
        };

        let cloned = result.clone();
        assert_eq!(result.doc_id, cloned.doc_id);
        assert_eq!(result.title, cloned.title);
        assert_eq!(result.snippet, cloned.snippet);
        assert_eq!(result.similarity, cloned.similarity);
    }

    #[test]
    fn test_semantic_search_result_debug() {
        let result = SemanticSearchResult {
            doc_id: "test.md".to_string(),
            title: "Test".to_string(),
            snippet: "Snippet".to_string(),
            similarity: 0.75,
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("test.md"));
        assert!(debug_str.contains("Test"));
        assert!(debug_str.contains("0.75"));
    }

    #[test]
    fn test_semantic_search_result_similarity_bounds() {
        // Test various similarity scores
        let low_score = SemanticSearchResult {
            doc_id: "low.md".to_string(),
            title: "Low".to_string(),
            snippet: "".to_string(),
            similarity: 0.01,
        };
        assert!(low_score.similarity > 0.0);

        let high_score = SemanticSearchResult {
            doc_id: "high.md".to_string(),
            title: "High".to_string(),
            snippet: "".to_string(),
            similarity: 0.99,
        };
        assert!(high_score.similarity < 1.0);

        let perfect_score = SemanticSearchResult {
            doc_id: "perfect.md".to_string(),
            title: "Perfect".to_string(),
            snippet: "".to_string(),
            similarity: 1.0,
        };
        assert_eq!(perfect_score.similarity, 1.0);
    }
}
