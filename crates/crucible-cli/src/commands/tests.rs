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
}
