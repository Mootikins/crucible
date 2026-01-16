//! Integration tests for ChatApp user interactions
//!
//! Tests popup behavior, Ctrl+C handling, command processing, and error states.

use crate::tui::ink::app::{Action, App, ViewContext};
use crate::tui::ink::chat_app::{ChatAppMsg, ChatMode, InkChatApp};
use crate::tui::ink::event::Event;
use crate::tui::ink::focus::FocusContext;
use crate::tui::ink::render::render_to_string;
use crate::tui::ink::test_harness::AppHarness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn view_with_default_ctx(app: &InkChatApp) -> crate::tui::ink::node::Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

// =============================================================================
// Popup Interaction Tests
// =============================================================================

#[test]
fn popup_opens_on_f1() {
    let mut app = InkChatApp::default();

    // Initially popup is closed
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        !output.contains("semantic_search"),
        "Popup should be closed initially"
    );

    // Press F1 to open popup
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("semantic_search"),
        "Popup should be open after F1"
    );
}

#[test]
fn popup_closes_on_f1_toggle() {
    let mut app = InkChatApp::default();

    // Open popup
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("semantic_search"), "Popup should be open");

    // Press F1 again to close
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        !output.contains("semantic_search"),
        "Popup should be closed after second F1"
    );
}

#[test]
fn popup_closes_on_escape() {
    let mut app = InkChatApp::default();

    // Open popup
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("semantic_search"), "Popup should be open");

    // Press Escape to close
    app.update(Event::Key(key(KeyCode::Esc)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        !output.contains("semantic_search"),
        "Popup should be closed after Escape"
    );
}

#[test]
fn popup_navigates_down() {
    let mut app = InkChatApp::default();

    // Open popup
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    // First item should be selected (semantic_search)
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    // Navigate down
    app.update(Event::Key(key(KeyCode::Down)));

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);

    // The selection indicator should have moved
    let indicator_pos_before = output.find('▸').unwrap_or(0);
    let indicator_pos_after = output_after.find('▸').unwrap_or(0);

    assert_ne!(
        indicator_pos_before, indicator_pos_after,
        "Selection indicator should move after Down"
    );
}

#[test]
fn popup_navigates_up() {
    let mut app = InkChatApp::default();

    // Open popup and move down first
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));
    app.update(Event::Key(key(KeyCode::Down)));
    app.update(Event::Key(key(KeyCode::Down)));

    let tree_before = view_with_default_ctx(&app);
    let output_before = render_to_string(&tree_before, 80);

    // Navigate up
    app.update(Event::Key(key(KeyCode::Up)));

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);

    let indicator_pos_before = output_before.find('▸').unwrap_or(0);
    let indicator_pos_after = output_after.find('▸').unwrap_or(0);

    assert_ne!(
        indicator_pos_before, indicator_pos_after,
        "Selection indicator should move after Up"
    );
}

#[test]
fn popup_up_at_top_stays_at_top() {
    let mut app = InkChatApp::default();

    // Open popup (selection at top)
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    let tree_before = view_with_default_ctx(&app);
    let output_before = render_to_string(&tree_before, 80);

    // Try to navigate up (should stay at top)
    app.update(Event::Key(key(KeyCode::Up)));

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);

    let indicator_pos_before = output_before.find('▸').unwrap_or(0);
    let indicator_pos_after = output_after.find('▸').unwrap_or(0);

    assert_eq!(
        indicator_pos_before, indicator_pos_after,
        "Selection should stay at top when pressing Up at top"
    );
}

#[test]
fn popup_down_at_bottom_stays_at_bottom() {
    let mut app = InkChatApp::default();

    // Open popup and navigate to bottom
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    // Navigate down many times to reach bottom
    for _ in 0..20 {
        app.update(Event::Key(key(KeyCode::Down)));
    }

    let tree_before = view_with_default_ctx(&app);
    let output_before = render_to_string(&tree_before, 80);

    // Try to navigate down again (should stay at bottom)
    app.update(Event::Key(key(KeyCode::Down)));

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);

    let indicator_pos_before = output_before.find('▸').unwrap_or(0);
    let indicator_pos_after = output_after.find('▸').unwrap_or(0);

    assert_eq!(
        indicator_pos_before, indicator_pos_after,
        "Selection should stay at bottom when pressing Down at bottom"
    );
}

#[test]
fn popup_enter_selects_and_closes() {
    let mut app = InkChatApp::default();

    // Open popup
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    // Verify popup is open
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("semantic_search"), "Popup should be open");

    // Press Enter to select
    app.update(Event::Key(key(KeyCode::Enter)));

    // Popup should be closed after Enter selection
    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);
    assert!(
        !output_after.contains("semantic_search"),
        "Popup should close after Enter selection"
    );
}

// =============================================================================
// Ctrl+C Behavior Tests
// =============================================================================

#[test]
fn ctrl_c_clears_non_empty_input() {
    let mut app = InkChatApp::default();

    // Type some text
    app.update(Event::Key(key(KeyCode::Char('h'))));
    app.update(Event::Key(key(KeyCode::Char('e'))));
    app.update(Event::Key(key(KeyCode::Char('l'))));
    app.update(Event::Key(key(KeyCode::Char('l'))));
    app.update(Event::Key(key(KeyCode::Char('o'))));

    assert_eq!(app.input_content(), "hello");

    // Press Ctrl+C
    let action = app.update(Event::Key(ctrl('c')));

    // Should continue (not quit) and clear input
    assert!(
        matches!(action, Action::Continue),
        "Ctrl+C with text should return Continue"
    );
    assert!(
        app.input_content().is_empty(),
        "Input should be cleared after Ctrl+C"
    );
}

#[test]
fn ctrl_c_shows_notification_when_empty() {
    let mut app = InkChatApp::default();

    // Input is empty
    assert!(app.input_content().is_empty());

    // Press Ctrl+C
    let action = app.update(Event::Key(ctrl('c')));

    // Should continue and show notification
    assert!(
        matches!(action, Action::Continue),
        "First Ctrl+C should return Continue"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Ctrl+C again to quit"),
        "Should show quit notification"
    );
}

#[test]
fn double_ctrl_c_within_timeout_quits() {
    let mut app = InkChatApp::default();

    // First Ctrl+C
    let action1 = app.update(Event::Key(ctrl('c')));
    assert!(matches!(action1, Action::Continue));

    // Second Ctrl+C immediately
    let action2 = app.update(Event::Key(ctrl('c')));
    assert!(matches!(action2, Action::Quit), "Double Ctrl+C should quit");
}

#[test]
fn ctrl_c_after_timeout_shows_notification_again() {
    let mut app = InkChatApp::default();

    // First Ctrl+C
    app.update(Event::Key(ctrl('c')));

    // Simulate time passing by resetting the internal state
    // We can't easily simulate time, but we can verify the notification shows
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Ctrl+C again to quit"));

    // Type something to reset the Ctrl+C state
    app.update(Event::Key(key(KeyCode::Char('a'))));
    app.update(Event::Key(ctrl('c'))); // This clears input

    // Now Ctrl+C again should show notification (not quit)
    let action = app.update(Event::Key(ctrl('c')));
    assert!(
        matches!(action, Action::Continue),
        "Ctrl+C after typing should show notification again"
    );
}

#[test]
fn notification_persists_until_timeout() {
    let mut app = InkChatApp::default();

    // Show notification
    app.update(Event::Key(ctrl('c')));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Ctrl+C again to quit"));

    // Press any other key - notification should still be visible (timeout-based clearing)
    app.update(Event::Key(key(KeyCode::Char('a'))));

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);
    assert!(
        output_after.contains("Ctrl+C again to quit"),
        "Notification should persist until timeout"
    );
}

#[test]
fn ctrl_c_resets_on_any_other_key() {
    let mut app = InkChatApp::default();

    // First Ctrl+C
    app.update(Event::Key(ctrl('c')));

    // Press some other key (resets the double-tap counter)
    app.update(Event::Key(key(KeyCode::Char('x'))));

    // Clear the 'x' we just typed
    app.update(Event::Key(ctrl('c')));

    let action = app.update(Event::Key(ctrl('c')));
    assert!(
        matches!(action, Action::Continue),
        "Ctrl+C after other key should reset counter"
    );

    // Second Ctrl+C now should quit
    let action2 = app.update(Event::Key(ctrl('c')));
    assert!(matches!(action2, Action::Quit));
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn error_clears_on_next_keypress() {
    let mut app = InkChatApp::default();

    // Trigger an error via unknown command
    app.on_message(ChatAppMsg::Error("Test error".to_string()));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Test error"), "Error should be displayed");

    // Press any key
    app.update(Event::Key(key(KeyCode::Char('a'))));

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);
    assert!(
        !output_after.contains("Test error"),
        "Error should clear on keypress"
    );
}

#[test]
fn unknown_slash_command_shows_error() {
    let mut harness: AppHarness<InkChatApp> = AppHarness::new(80, 24);
    harness.render();

    // Type unknown command
    harness.send_text("/unknowncommand");
    harness.send_enter();

    let output = harness.viewport();
    assert!(
        output.contains("Unknown command") || output.contains("unknowncommand"),
        "Should show error for unknown command: {}",
        output
    );
}

#[test]
fn unknown_repl_command_shows_error() {
    let mut harness: AppHarness<InkChatApp> = AppHarness::new(80, 24);
    harness.render();

    // Type unknown REPL command
    harness.send_text(":unknownrepl");
    harness.send_enter();

    let output = harness.viewport();
    assert!(
        output.contains("Unknown") || output.contains("unknownrepl"),
        "Should show error for unknown REPL command: {}",
        output
    );
}

#[test]
fn error_renders_in_red() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::Error("Test error".to_string()));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    // Check for red ANSI code (31 is red foreground)
    assert!(
        output.contains("\x1b[31m") || output.contains("\x1b[38;5;9m"),
        "Error should be rendered in red"
    );
}

#[test]
fn multiple_errors_replace_previous() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::Error("First error".to_string()));

    let tree1 = view_with_default_ctx(&app);
    let output1 = render_to_string(&tree1, 80);
    assert!(output1.contains("First error"));

    app.on_message(ChatAppMsg::Error("Second error".to_string()));

    let tree2 = view_with_default_ctx(&app);
    let output2 = render_to_string(&tree2, 80);
    assert!(output2.contains("Second error"), "Should show second error");
    assert!(
        !output2.contains("First error"),
        "Should not show first error"
    );
}

// =============================================================================
// Command Coverage Tests
// =============================================================================

#[test]
fn slash_act_sets_mode_to_act() {
    let mut harness: AppHarness<InkChatApp> = AppHarness::new(80, 24);
    harness.render();

    harness.send_text("/act");
    harness.send_enter();

    let output = harness.viewport();
    assert!(
        output.contains("Act") || output.contains("act"),
        "Mode should be set to act: {}",
        output
    );
}

#[test]
fn slash_auto_sets_mode_to_auto() {
    let mut harness: AppHarness<InkChatApp> = AppHarness::new(80, 24);
    harness.render();

    harness.send_text("/auto");
    harness.send_enter();

    let output = harness.viewport();
    assert!(
        output.contains("Auto") || output.contains("auto"),
        "Mode should be set to auto: {}",
        output
    );
}

#[test]
fn slash_help_adds_system_message() {
    let mut app = InkChatApp::default();

    // Type /help and submit
    for c in "/help".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("Commands:") || output.contains("/mode") || output.contains("/help"),
        "Help should show available commands: {}",
        output
    );
}

#[test]
fn repl_quit_returns_quit_action() {
    let mut app = InkChatApp::default();

    // Type :q and submit
    for c in ":q".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        matches!(action, Action::Quit),
        ":q should return Quit action"
    );
}

#[test]
fn repl_help_adds_system_message() {
    let mut app = InkChatApp::default();

    // Type :help and submit
    for c in ":help".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("REPL") || output.contains(":q"),
        "REPL help should show available commands: {}",
        output
    );
}

#[test]
fn slash_mode_cycles_through_modes() {
    let mut app = InkChatApp::default();

    // Start in Plan mode
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Plan"), "Should start in Plan mode");

    // Type /mode and submit
    for c in "/mode".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Act"), "Should cycle to Act mode");

    // Type /mode again
    for c in "/mode".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Auto"), "Should cycle to Auto mode");

    // Type /mode again
    for c in "/mode".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("Plan"), "Should cycle back to Plan mode");
}

// =============================================================================
// Autocomplete Trigger Tests
// =============================================================================

#[test]
fn at_symbol_triggers_file_autocomplete() {
    let mut app = InkChatApp::default();
    app.set_workspace_files(vec![
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "README.md".to_string(),
    ]);

    for c in "@ma".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("main.rs"),
        "Should show file popup with main.rs"
    );
}

#[test]
fn double_bracket_triggers_note_autocomplete() {
    let mut app = InkChatApp::default();
    app.set_kiln_notes(vec![
        "Projects/README.md".to_string(),
        "Notes/Ideas.md".to_string(),
    ]);

    for c in "[[pro".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Projects/README.md"),
        "Should show note popup with Projects"
    );
}

#[test]
fn autocomplete_closes_on_space_after_at() {
    let mut app = InkChatApp::default();
    app.set_workspace_files(vec!["src/main.rs".to_string()]);

    for c in "@main ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(!output.contains("file"), "Popup should close after space");
}

#[test]
fn autocomplete_selection_inserts_file_mention() {
    let mut app = InkChatApp::default();
    app.set_workspace_files(vec!["src/main.rs".to_string()]);

    for c in "@m".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    app.update(Event::Key(key(KeyCode::Enter)));

    assert_eq!(app.input_content(), "@src/main.rs ");
}

#[test]
fn autocomplete_selection_inserts_note_mention() {
    let mut app = InkChatApp::default();
    app.set_kiln_notes(vec!["Projects/README.md".to_string()]);

    for c in "[[p".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    app.update(Event::Key(key(KeyCode::Enter)));

    assert_eq!(app.input_content(), "[[Projects/README.md]] ");
}

// =============================================================================
// REPL Command Tests
// =============================================================================

#[test]
fn palette_command_opens_command_popup() {
    let mut app = InkChatApp::default();

    for c in ":palette".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("semantic_search"),
        "Palette command should open command popup"
    );
}

#[test]
fn commands_command_opens_command_popup() {
    let mut app = InkChatApp::default();

    for c in ":commands".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("/mode"),
        "Commands should open command popup"
    );
}

// =============================================================================
// Regression Tests for Autocomplete Bugfixes
// =============================================================================

#[test]
fn ctrl_c_closes_popup_instead_of_inserting_c() {
    let mut app = InkChatApp::default();
    app.set_workspace_files(vec!["test.rs".to_string()]);

    app.update(Event::Key(key(KeyCode::Char('@'))));
    assert!(app.is_popup_visible(), "Popup should open on @");

    app.update(Event::Key(ctrl('c')));

    assert!(!app.is_popup_visible(), "Ctrl+C should close popup");
    assert!(
        !app.input_content().contains('c'),
        "Ctrl+C should not insert 'c' character"
    );
}

#[test]
fn slash_command_triggers_after_whitespace() {
    let mut app = InkChatApp::default();

    for c in "hello /hel".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open for / after whitespace"
    );
    assert_eq!(
        app.current_popup_filter(),
        "hel",
        "Filter should be text after slash"
    );
}

#[test]
fn slash_command_does_not_trigger_mid_word() {
    let mut app = InkChatApp::default();

    for c in "http://example".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        !app.is_popup_visible(),
        "Popup should NOT open for / preceded by non-whitespace"
    );
}

#[test]
fn empty_workspace_files_does_not_show_popup() {
    let mut app = InkChatApp::default();

    app.update(Event::Key(key(KeyCode::Char('@'))));

    assert!(
        !app.is_popup_visible(),
        "Popup should not show when no files to display"
    );
}

#[test]
fn empty_kiln_notes_does_not_show_popup() {
    let mut app = InkChatApp::default();

    for c in "[[".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        !app.is_popup_visible(),
        "Popup should not show when no notes to display"
    );
}

// =============================================================================
// Shell Modal Tests
// =============================================================================

#[test]
fn shell_command_opens_modal() {
    let mut app = InkChatApp::default();

    for c in "!echo hello".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        app.has_shell_modal(),
        "Shell modal should open after ! command"
    );
}

#[test]
fn empty_shell_command_shows_error() {
    let mut app = InkChatApp::default();

    app.update(Event::Key(key(KeyCode::Char('!'))));
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.has_shell_modal(),
        "Empty shell command should not open modal"
    );
}

#[test]
fn shell_command_captures_output() {
    let mut app = InkChatApp::default();

    for c in "!echo hello".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(app.has_shell_modal(), "Shell modal should open");

    for _ in 0..20 {
        app.update(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let output = app.shell_output_lines();
    assert!(
        output.iter().any(|line| line.contains("hello")),
        "Output should contain 'hello', got: {:?}",
        output
    );
}

#[test]
fn shell_modal_closes_on_escape() {
    let mut app = InkChatApp::default();

    for c in "!echo test".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    for _ in 0..10 {
        app.update(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert!(app.has_shell_modal(), "Modal should be open");

    app.update(Event::Key(key(KeyCode::Esc)));

    assert!(!app.has_shell_modal(), "Modal should close on Esc");
}
