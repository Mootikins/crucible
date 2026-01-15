//! Tests for new REPL commands (:palette, :rewind, :write, :undo, :context, :rename)

use super::fixtures::{registries, sessions};
use super::{Harness, TEST_HEIGHT, TEST_WIDTH};
use crate::tui::state::PopupKind;
use crossterm::event::KeyCode;
use insta::assert_snapshot;

// =============================================================================
// :palette command tests
// =============================================================================

mod palette_tests {
    use super::*;

    #[test]
    fn palette_popup_shows_all_repl_commands() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        assert!(h.has_popup());
        assert_snapshot!("palette_popup_open", h.render());
    }

    #[test]
    fn palette_popup_navigation() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        h.key(KeyCode::Down);
        h.key(KeyCode::Down);

        assert_eq!(h.popup_selected(), Some(2));
        assert_snapshot!("palette_popup_navigated", h.render());
    }

    #[test]
    fn palette_popup_filter() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        h.keys("mod");

        assert_eq!(h.popup_query(), Some("mod"));
        assert_snapshot!("palette_popup_filtered", h.render());
    }

    #[test]
    fn palette_popup_select_inserts_command() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        h.key(KeyCode::Enter);

        assert!(!h.has_popup());
        assert!(h.input_text().starts_with(':'));
        assert_snapshot!("palette_popup_selected", h.render());
    }

    #[test]
    fn palette_escape_closes_popup() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        h.key(KeyCode::Esc);

        assert!(!h.has_popup());
        assert!(h.input_text().is_empty());
    }
}

// =============================================================================
// :rewind command tests (ConversationState::rewind)
// =============================================================================

mod rewind_tests {
    use super::*;

    #[test]
    fn rewind_removes_messages() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multi_turn());

        assert_eq!(h.conversation_len(), 4);

        h.view.state_mut().conversation.rewind(2);

        assert_eq!(h.conversation_len(), 2);
        assert_snapshot!("rewind_after_2", h.render());
    }

    #[test]
    fn rewind_all_messages() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        assert_eq!(h.conversation_len(), 2);

        let rewound = h.view.state_mut().conversation.rewind(10);

        assert_eq!(rewound, 2);
        assert_eq!(h.conversation_len(), 0);
        assert_snapshot!("rewind_all_empty", h.render());
    }

    #[test]
    fn rewind_returns_count() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multi_turn());

        let rewound = h.view.state_mut().conversation.rewind(2);
        assert_eq!(rewound, 2);

        let rewound = h.view.state_mut().conversation.rewind(100);
        assert_eq!(rewound, 2);
    }

    #[test]
    fn rewind_empty_returns_zero() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        let rewound = h.view.state_mut().conversation.rewind(5);
        assert_eq!(rewound, 0);
    }

    #[test]
    fn rewind_preserves_earlier_messages() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multi_turn());

        h.view.state_mut().conversation.rewind(2);

        let items = h.conversation_items();
        assert_eq!(items.len(), 2);

        if let crate::tui::conversation::ConversationItem::UserMessage { content } = &items[0] {
            assert_eq!(content, "What is Crucible?");
        } else {
            panic!("Expected user message");
        }
    }
}

// =============================================================================
// Input mode color tests
// =============================================================================

mod input_mode_tests {
    use super::*;

    #[test]
    fn command_mode_triggered_by_colon() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.key(KeyCode::Char(':'));

        assert!(h.has_popup());
        assert_eq!(h.input_text(), ":");
        assert_snapshot!("input_mode_command", h.render());
    }

    #[test]
    fn slash_triggers_command_popup() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.key(KeyCode::Char('/'));

        assert!(h.has_popup());
        assert_eq!(h.input_text(), "/");
        assert_snapshot!("input_mode_slash", h.render());
    }

    #[test]
    fn at_triggers_agent_file_popup() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.key(KeyCode::Char('@'));

        assert!(h.has_popup());
        assert_eq!(h.input_text(), "@");
    }

    #[test]
    fn normal_text_no_popup() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.keys("hello world");

        assert!(!h.has_popup());
        assert_eq!(h.input_text(), "hello world");
    }
}

// =============================================================================
// Conversation display with various states
// =============================================================================

mod conversation_display_tests {
    use super::*;

    #[test]
    fn empty_conversation() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        assert_eq!(h.conversation_len(), 0);
        assert_snapshot!("conversation_empty", h.render());
    }

    #[test]
    fn basic_exchange_display() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        assert_eq!(h.conversation_len(), 2);
        assert_snapshot!("conversation_basic", h.render());
    }

    #[test]
    fn multi_turn_display() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multi_turn());

        assert_eq!(h.conversation_len(), 4);
        assert_snapshot!("conversation_multi_turn", h.render());
    }

    #[test]
    fn with_tool_calls_display() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_tool_calls());

        assert_snapshot!("conversation_with_tools", h.render());
    }
}

// =============================================================================
// Extended REPL command fixtures
// =============================================================================

mod extended_repl_fixtures {
    use super::*;
    use crate::tui::state::PopupItem;

    fn all_new_repl_commands() -> Vec<PopupItem> {
        vec![
            registries::repl("quit", "Exit the application"),
            registries::repl("help", "Show help"),
            registries::repl("palette", "Open command palette"),
            registries::repl("write", "Export conversation to file"),
            registries::repl("rename", "Rename current session"),
            registries::repl("rewind", "Revert conversation messages"),
            registries::repl("context", "Show attached context"),
            registries::repl("undo", "Show conversation undo tree"),
            registries::repl("model", "Switch model"),
            registries::repl("mode", "Cycle session mode"),
        ]
    }

    #[test]
    fn all_new_commands_in_popup() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, all_new_repl_commands());

        assert!(h.has_popup());
        assert_snapshot!("all_repl_commands", h.render());
    }

    #[test]
    fn filter_to_rewind() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, all_new_repl_commands());

        h.keys("rew");

        assert_snapshot!("filter_rewind", h.render());
    }

    #[test]
    fn filter_to_write() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, all_new_repl_commands());

        h.keys("wri");

        assert_snapshot!("filter_write", h.render());
    }
}

// =============================================================================
// Conversation tree branching tests
// =============================================================================

mod tree_branching_tests {
    use super::*;

    #[test]
    fn rewind_creates_branches_on_new_messages() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multi_turn());

        assert_eq!(h.conversation_len(), 4);
        assert!(!h.view.state().conversation.has_branches());

        h.view.state_mut().conversation.rewind(2);
        assert_eq!(h.conversation_len(), 2);

        h.view
            .state_mut()
            .conversation
            .push_user_message("A different question");
        assert_eq!(h.conversation_len(), 3);
        assert!(h.view.state().conversation.has_branches());

        let summary = h.view.state().conversation.tree_summary();
        assert_eq!(summary.total_nodes, 5);
        assert_eq!(summary.current_depth, 3);
        assert_eq!(summary.branch_points, 1);
    }

    #[test]
    fn linear_conversation_has_no_branches() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::multi_turn());

        assert!(!h.view.state().conversation.has_branches());
        let summary = h.view.state().conversation.tree_summary();
        assert_eq!(summary.total_nodes, 4);
        assert_eq!(summary.branch_points, 0);
    }

    #[test]
    fn tree_summary_tracks_depth() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.view.state_mut().conversation.push_user_message("Hello");
        let s = h.view.state().conversation.tree_summary();
        assert_eq!(s.current_depth, 1);
        assert_eq!(s.total_nodes, 1);

        h.view.state_mut().conversation.push_assistant_message("Hi");
        let s = h.view.state().conversation.tree_summary();
        assert_eq!(s.current_depth, 2);
        assert_eq!(s.total_nodes, 2);
    }
}
