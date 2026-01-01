//! Snapshot tests for popup rendering and behavior
//!
//! These tests capture and verify how popups are displayed in the TUI,
//! including command popups, agent/file popups, and popup effects.

use super::fixtures::registries;
use super::Harness;
use crate::tui::action_dispatch::{
    popup_item_to_effect, PopupEffect, PopupHook, PopupHooks,
};
use crate::tui::state::{PopupItem, PopupKind};
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
        ];

        for item in items {
            let effect = popup_item_to_effect(&item);
            // All effects should be valid (not panic)
            match effect {
                PopupEffect::InsertToken { token } => assert!(!token.is_empty()),
                PopupEffect::AddFileContext { path } => assert!(!path.is_empty()),
                PopupEffect::AddNoteContext { path } => assert!(!path.is_empty()),
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
        assert_eq!(popup.items.len(), 3);
        assert_eq!(popup.kind, PopupKind::AgentOrFile);
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
        let mut h = Harness::new(80, 24)
            .with_session(sessions::basic_exchange());

        h.key(KeyCode::Char('/'));
        // Manually add items since slash just triggers popup, doesn't populate
        h.state.popup.as_mut().unwrap().items = registries::minimal_commands();
        h.view.set_popup(h.state.popup.clone());

        assert_snapshot!("popup_over_conversation", h.render());
    }

    #[test]
    fn popup_does_not_affect_conversation_state() {
        let h1 = Harness::new(80, 24)
            .with_session(sessions::basic_exchange());
        let initial_len = h1.conversation_len();

        let mut h2 = Harness::new(80, 24)
            .with_session(sessions::basic_exchange());
        h2.key(KeyCode::Char('/'));
        h2.keys("test");
        h2.key(KeyCode::Esc);

        // Conversation should be unchanged
        assert_eq!(h2.conversation_len(), initial_len);
    }
}
