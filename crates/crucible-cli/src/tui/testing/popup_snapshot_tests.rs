//! Snapshot tests for popup rendering and behavior
//!
//! These tests capture and verify how popups are displayed in the TUI,
//! including command popups, agent/file popups, and popup effects.

use super::fixtures::registries;
use super::Harness;
use crate::tui::action_dispatch::{popup_item_to_effect, PopupEffect, PopupHook, PopupHooks};
use crate::tui::state::types::{PopupItem, PopupKind};
use crossterm::event::KeyCode;
use insta::assert_snapshot;

// =============================================================================
// Snapshot Tests - Popup Rendering
// =============================================================================

mod snapshots {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    #[test]
    fn popup_command_open() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert_snapshot!("popup_command_open", h.render());
    }

    #[test]
    fn popup_command_minimal() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::minimal_commands());

        assert_snapshot!("popup_command_minimal", h.render());
    }

    #[test]
    fn popup_agent_list() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::test_agents());

        assert_snapshot!("popup_agent_list", h.render());
    }

    #[test]
    fn popup_file_list() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::test_files());

        assert_snapshot!("popup_file_list", h.render());
    }

    #[test]
    fn popup_note_list() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::test_notes());

        assert_snapshot!("popup_note_list", h.render());
    }

    #[test]
    fn popup_skills_list() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::test_skills());

        assert_snapshot!("popup_skills_list", h.render());
    }

    #[test]
    fn popup_repl_commands() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        assert_snapshot!("popup_repl_commands", h.render());
    }

    #[test]
    fn popup_mixed_items() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::mixed_agent_file_items());

        assert_snapshot!("popup_mixed_items", h.render());
    }

    #[test]
    fn popup_navigation_second_item() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        h.key(KeyCode::Down);
        assert_snapshot!("popup_navigation_second", h.render());
    }

    #[test]
    fn popup_navigation_third_item() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        h.key(KeyCode::Down);
        h.key(KeyCode::Down);
        assert_snapshot!("popup_navigation_third", h.render());
    }

    #[test]
    fn popup_navigation_wrap_to_top() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::minimal_commands());

        // Go down past last item, should wrap (or stay at last)
        h.key(KeyCode::Down);
        h.key(KeyCode::Down);
        h.key(KeyCode::Down); // past the 2 items
        assert_snapshot!("popup_navigation_wrap", h.render());
    }

    #[test]
    fn popup_with_scroll() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::many_commands());

        assert_snapshot!("popup_many_commands", h.render());
    }

    #[test]
    fn popup_scrolled_down() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::many_commands());

        // Navigate down several items to trigger scrolling
        for _ in 0..10 {
            h.key(KeyCode::Down);
        }
        assert_snapshot!("popup_scrolled_down", h.render());
    }
}

// =============================================================================
// Unit Tests - PopupEffect Conversion
// =============================================================================

mod popup_effect_tests {
    use super::*;

    #[test]
    fn command_to_insert_token() {
        let item = PopupItem::cmd("search").desc("Search notes");
        let effect = popup_item_to_effect(&item);

        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/search ".into()
            }
        );
    }

    #[test]
    fn agent_to_insert_token() {
        let item = PopupItem::agent("researcher").desc("Research agent");
        let effect = popup_item_to_effect(&item);

        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "@researcher".into()
            }
        );
    }

    #[test]
    fn file_to_add_context() {
        let item = PopupItem::file("src/main.rs");
        let effect = popup_item_to_effect(&item);

        assert_eq!(
            effect,
            PopupEffect::AddFileContext {
                path: "src/main.rs".into()
            }
        );
    }

    #[test]
    fn note_to_add_context() {
        let item = PopupItem::note("Projects/Crucible");
        let effect = popup_item_to_effect(&item);

        assert_eq!(
            effect,
            PopupEffect::AddNoteContext {
                path: "Projects/Crucible".into()
            }
        );
    }

    #[test]
    fn skill_to_insert_token() {
        let item = PopupItem::skill("commit").desc("Create git commit");
        let effect = popup_item_to_effect(&item);

        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/commit ".into()
            }
        );
    }

    #[test]
    fn all_item_types_produce_effects() {
        // Ensure every PopupItem variant produces a PopupEffect
        let items = vec![
            PopupItem::cmd("test"),
            PopupItem::agent("test"),
            PopupItem::file("test"),
            PopupItem::note("test"),
            PopupItem::skill("test"),
            PopupItem::repl("quit"),
            PopupItem::session("sess_123"),
        ];

        for item in items {
            let effect = popup_item_to_effect(&item);
            // All effects should be valid (not panic)
            match effect {
                PopupEffect::InsertToken { token } => assert!(!token.is_empty()),
                PopupEffect::AddFileContext { path } => assert!(!path.is_empty()),
                PopupEffect::AddNoteContext { path } => assert!(!path.is_empty()),
                PopupEffect::ExecuteReplCommand { name } => assert!(!name.is_empty()),
                PopupEffect::ResumeSession { session_id } => assert!(!session_id.is_empty()),
            }
        }
    }
}

// =============================================================================
// Unit Tests - PopupHooks (Scriptable Override)
// =============================================================================

mod popup_hooks_tests {
    use super::*;

    /// Mock hook that always returns a custom effect
    struct OverrideHook {
        effect: PopupEffect,
    }

    impl PopupHook for OverrideHook {
        fn on_popup_select(&self, _item: &PopupItem) -> Option<PopupEffect> {
            Some(self.effect.clone())
        }
    }

    /// Mock hook that returns None (fall through to default)
    struct PassthroughHook;

    impl PopupHook for PassthroughHook {
        fn on_popup_select(&self, _item: &PopupItem) -> Option<PopupEffect> {
            None
        }
    }

    /// Mock hook that only handles files
    struct FileOnlyHook {
        replacement: PopupEffect,
    }

    impl PopupHook for FileOnlyHook {
        fn on_popup_select(&self, item: &PopupItem) -> Option<PopupEffect> {
            match item {
                PopupItem::File { .. } => Some(self.replacement.clone()),
                _ => None,
            }
        }
    }

    #[test]
    fn hooks_empty_uses_default() {
        let hooks = PopupHooks::new();
        let item = PopupItem::cmd("test");
        let effect = hooks.dispatch(&item);

        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/test ".into()
            }
        );
    }

    #[test]
    fn hook_can_override_default() {
        let mut hooks = PopupHooks::new();
        hooks.register(Box::new(OverrideHook {
            effect: PopupEffect::InsertToken {
                token: "OVERRIDDEN".into(),
            },
        }));

        let item = PopupItem::file("test.rs");
        let effect = hooks.dispatch(&item);

        // Hook overrides the file's default AddFileContext
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "OVERRIDDEN".into()
            }
        );
    }

    #[test]
    fn passthrough_hook_uses_default() {
        let mut hooks = PopupHooks::new();
        hooks.register(Box::new(PassthroughHook));

        let item = PopupItem::note("Projects/Test");
        let effect = hooks.dispatch(&item);

        // PassthroughHook returns None, so default is used
        assert_eq!(
            effect,
            PopupEffect::AddNoteContext {
                path: "Projects/Test".into()
            }
        );
    }

    #[test]
    fn hooks_first_match_wins() {
        let mut hooks = PopupHooks::new();

        // First hook: override for files only
        hooks.register(Box::new(FileOnlyHook {
            replacement: PopupEffect::InsertToken {
                token: "FILE_HOOK".into(),
            },
        }));

        // Second hook: would override everything (but shouldn't be reached for files)
        hooks.register(Box::new(OverrideHook {
            effect: PopupEffect::InsertToken {
                token: "CATCH_ALL".into(),
            },
        }));

        // File should be handled by first hook
        let file_item = PopupItem::file("test.rs");
        let file_effect = hooks.dispatch(&file_item);
        assert_eq!(
            file_effect,
            PopupEffect::InsertToken {
                token: "FILE_HOOK".into()
            }
        );

        // Non-file passes through first hook, caught by second
        let cmd_item = PopupItem::cmd("test");
        let cmd_effect = hooks.dispatch(&cmd_item);
        assert_eq!(
            cmd_effect,
            PopupEffect::InsertToken {
                token: "CATCH_ALL".into()
            }
        );
    }

    #[test]
    fn multiple_passthrough_hooks_fall_to_default() {
        let mut hooks = PopupHooks::new();
        hooks.register(Box::new(PassthroughHook));
        hooks.register(Box::new(PassthroughHook));
        hooks.register(Box::new(PassthroughHook));

        let item = PopupItem::skill("commit").desc("Git commit");
        let effect = hooks.dispatch(&item);

        // All hooks pass through, default is used
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "/commit ".into()
            }
        );
    }

    #[test]
    fn hook_can_transform_effect_type() {
        // A hook can change a File from AddFileContext to InsertToken
        let mut hooks = PopupHooks::new();
        hooks.register(Box::new(FileOnlyHook {
            replacement: PopupEffect::InsertToken {
                token: "@file:custom".into(),
            },
        }));

        let item = PopupItem::file("src/lib.rs");
        let effect = hooks.dispatch(&item);

        // File was transformed from AddFileContext to InsertToken
        assert_eq!(
            effect,
            PopupEffect::InsertToken {
                token: "@file:custom".into()
            }
        );
    }
}

// =============================================================================
// Integration Tests - Harness Popup Behavior
// =============================================================================

mod harness_popup_tests {
    use super::*;

    #[test]
    fn slash_opens_command_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));

        assert!(h.has_popup());
        assert_eq!(h.input_text(), "/");
    }

    #[test]
    fn at_opens_agent_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('@'));

        assert!(h.has_popup());
        assert_eq!(h.input_text(), "@");
    }

    #[test]
    fn escape_closes_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        assert!(h.has_popup());

        h.key(KeyCode::Esc);
        assert!(!h.has_popup());
    }

    #[test]
    fn typing_updates_query() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        h.keys("sea");

        assert_eq!(h.popup_query(), Some("sea"));
    }

    #[test]
    fn backspace_removes_query_chars() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        h.keys("search");
        h.key(KeyCode::Backspace);
        h.key(KeyCode::Backspace);

        assert_eq!(h.popup_query(), Some("sear"));
    }

    #[test]
    fn backspace_on_empty_query_closes_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        assert!(h.has_popup());

        h.key(KeyCode::Backspace);
        assert!(!h.has_popup());
    }

    #[test]
    fn navigation_changes_selection() {
        let mut h = Harness::new(80, 24)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert_eq!(h.popup_selected(), Some(0));

        h.key(KeyCode::Down);
        assert_eq!(h.popup_selected(), Some(1));

        h.key(KeyCode::Down);
        assert_eq!(h.popup_selected(), Some(2));

        h.key(KeyCode::Up);
        assert_eq!(h.popup_selected(), Some(1));
    }

    #[test]
    fn enter_confirms_selection_and_closes() {
        let mut h = Harness::new(80, 24)
            .with_popup_items(PopupKind::Command, registries::minimal_commands());

        h.key(KeyCode::Enter);

        assert!(!h.has_popup());
        // Should have inserted the token
        assert!(h.input_text().starts_with('/'));
    }

    #[test]
    fn enter_with_navigation_selects_correct_item() {
        let mut h = Harness::new(80, 24)
            .with_popup_items(PopupKind::Command, registries::minimal_commands());

        // First item is "search", second is "help"
        h.key(KeyCode::Down);
        h.key(KeyCode::Enter);

        assert!(!h.has_popup());
        // Should have inserted the second item's token
        assert_eq!(h.input_text(), "/help ");
    }

    #[test]
    fn popup_items_accessible_via_harness() {
        let h = Harness::new(80, 24)
            .with_popup_items(PopupKind::AgentOrFile, registries::test_agents());

        let popup = h.popup().expect("should have popup");
        assert_eq!(popup.items().len(), 3);
        assert_eq!(popup.kind(), PopupKind::AgentOrFile);
    }
}

// =============================================================================
// Snapshot Tests - Popup with Conversation
// =============================================================================

mod popup_with_context_tests {
    use super::*;
    use crate::tui::testing::fixtures::sessions;

    #[test]
    fn popup_over_conversation() {
        // Create harness with popup items pre-populated
        let h = Harness::new(80, 24)
            .with_session(sessions::basic_exchange())
            .with_popup_items(PopupKind::Command, registries::minimal_commands());

        assert_snapshot!("popup_over_conversation", h.render());
    }

    #[test]
    fn popup_does_not_affect_conversation_state() {
        let h1 = Harness::new(80, 24).with_session(sessions::basic_exchange());
        let initial_len = h1.conversation_len();

        let mut h2 = Harness::new(80, 24).with_session(sessions::basic_exchange());
        h2.key(KeyCode::Char('/'));
        h2.keys("test");
        h2.key(KeyCode::Esc);

        // Conversation should be unchanged
        assert_eq!(h2.conversation_len(), initial_len);
    }
}

// =============================================================================
// Realistic Workflow Tests - User scenarios
// =============================================================================

mod workflow_tests {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// User types `/se` to filter commands, sees filtered results
    #[test]
    fn workflow_filter_commands() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        // Type filter text
        h.keys("se");

        // Query should update
        assert_eq!(h.popup_query(), Some("se"));

        // Snapshot shows filtered state
        assert_snapshot!("workflow_filter_commands", h.render());
    }

    /// User types `@re` to filter agents
    #[test]
    fn workflow_filter_agents() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::test_agents());

        // Type filter for "researcher"
        h.keys("re");

        assert_eq!(h.popup_query(), Some("re"));
        assert_snapshot!("workflow_filter_agents", h.render());
    }

    /// User types `:` to open REPL popup, then navigates and selects
    #[test]
    fn workflow_repl_navigate_select() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::ReplCommand, registries::test_repl_commands());

        // Navigate to third item
        h.key(KeyCode::Down);
        h.key(KeyCode::Down);

        assert_eq!(h.popup_selected(), Some(2));
        assert_snapshot!("workflow_repl_navigate", h.render());

        // Select it
        h.key(KeyCode::Enter);

        // Popup should close, input should have the token
        assert!(!h.has_popup());
        assert!(h.input_text().starts_with(':'));
    }

    /// Complete flow: open popup, type filter, navigate, select
    #[test]
    fn workflow_complete_command_selection() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        // Step 1: Initial state with popup open
        assert!(h.has_popup());
        assert_eq!(h.popup_selected(), Some(0));

        // Step 2: Type filter
        h.keys("hel");
        assert_eq!(h.popup_query(), Some("hel"));

        // Step 3: Navigate to confirm selection
        h.key(KeyCode::Down);
        h.key(KeyCode::Up);

        // Step 4: Confirm
        h.key(KeyCode::Enter);

        // Popup closes, input has token
        assert!(!h.has_popup());
        // Should have a command token (depends on filter matching)
        assert!(h.input_text().starts_with('/'));
    }

    /// User opens popup and immediately cancels
    #[test]
    fn workflow_cancel_immediately() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert!(h.has_popup());

        h.key(KeyCode::Esc);

        assert!(!h.has_popup());
        // Input should be cleared on cancel
        assert_eq!(h.input_text(), "");
    }

    /// User uses backspace to remove filter characters
    #[test]
    fn workflow_backspace_filter() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        // Type and then backspace
        h.keys("search");
        assert_eq!(h.popup_query(), Some("search"));

        h.key(KeyCode::Backspace);
        h.key(KeyCode::Backspace);
        assert_eq!(h.popup_query(), Some("sear"));

        assert_snapshot!("workflow_backspace_filter", h.render());
    }

    /// Full navigation cycle: down past end wraps to top
    #[test]
    fn workflow_navigation_wrap() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::minimal_commands()); // 2 items

        // Start at 0
        assert_eq!(h.popup_selected(), Some(0));

        // Down -> 1
        h.key(KeyCode::Down);
        assert_eq!(h.popup_selected(), Some(1));

        // Down again should wrap or stay (depending on implementation)
        h.key(KeyCode::Down);
        // Either wraps to 0 or stays at last
        let selected = h.popup_selected().unwrap();
        assert!(selected <= 1);

        assert_snapshot!("workflow_navigation_wrap", h.render());
    }

    /// Keyboard navigation preserves query while moving selection
    #[test]
    fn workflow_navigate_while_filtered() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        // Type filter
        h.keys("s");

        // Navigate
        h.key(KeyCode::Down);

        // Query should be preserved
        assert_eq!(h.popup_query(), Some("s"));
        assert!(h.has_popup());

        assert_snapshot!("workflow_navigate_while_filtered", h.render());
    }

    /// Multiple @ mentions in a row (close one, open another)
    #[test]
    fn workflow_multiple_triggers() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        // First @ trigger - opens popup with EmptyProvider (no items)
        h.key(KeyCode::Char('@'));
        assert!(h.has_popup());

        // Cancel
        h.key(KeyCode::Esc);
        assert!(!h.has_popup());

        // Second @ trigger
        h.key(KeyCode::Char('@'));
        assert!(h.has_popup());

        // Type some text
        h.keys("test");
        assert_eq!(h.popup_query(), Some("test"));
    }

    /// Popup with long item descriptions renders correctly
    #[test]
    fn workflow_long_descriptions() {
        // Create items with long descriptions
        let items = vec![
            registries::command(
                "search",
                "Search across all notes in your vault using semantic similarity",
            ),
            registries::command(
                "help",
                "Display comprehensive help for all available commands and features",
            ),
            registries::command(
                "clear",
                "Clear the current conversation history and start fresh",
            ),
        ];

        let h = Harness::new(WIDTH, HEIGHT).with_popup_items(PopupKind::Command, items);

        assert_snapshot!("workflow_long_descriptions", h.render());
    }
}

// =============================================================================
// Snapshot Tests - Session Popup (/resume command)
// =============================================================================

mod session_popup_tests {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    #[test]
    fn popup_session_list() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Session, registries::test_sessions());

        assert_snapshot!("popup_session_list", h.render());
    }

    #[test]
    fn popup_session_many() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Session, registries::many_sessions());

        assert_snapshot!("popup_session_many", h.render());
    }

    #[test]
    fn popup_session_navigation() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Session, registries::test_sessions());

        h.key(KeyCode::Down);
        assert_snapshot!("popup_session_second", h.render());
    }

    #[test]
    fn popup_session_over_conversation() {
        use crate::tui::testing::fixtures::sessions;

        let h = Harness::new(WIDTH, HEIGHT)
            .with_session(sessions::basic_exchange())
            .with_popup_items(PopupKind::Session, registries::test_sessions());

        assert_snapshot!("popup_session_over_conversation", h.render());
    }

    // ==========================================================================
    // Unit Tests - Session PopupItem
    // ==========================================================================

    #[test]
    fn session_item_has_correct_kind() {
        let item = registries::session("test-123", "Test session");
        assert!(item.is_session());
    }

    #[test]
    fn session_item_token_is_id() {
        let item = registries::session("chat-2025-01-01-abc", "Test session");
        assert_eq!(item.token(), "chat-2025-01-01-abc");
    }

    #[test]
    fn session_popup_effect() {
        let item = registries::session("chat-2025-01-01-abc", "Test session");
        let effect = popup_item_to_effect(&item);
        assert_eq!(
            effect,
            PopupEffect::ResumeSession {
                session_id: "chat-2025-01-01-abc".into()
            }
        );
    }
}
