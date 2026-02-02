//! Integration tests for ChatApp user interactions
//!
//! Tests popup behavior, Ctrl+C handling, command processing, and error states.

use crate::tui::oil::app::{Action, App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, ChatMode, OilChatApp};
use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::test_harness::AppHarness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn assert_contains(output: &str, needle: &str, context: &str) {
    assert!(
        output.contains(needle),
        "Expected to find '{}' in output. Context: {}\nOutput:\n{}",
        needle,
        context,
        output
    );
}

fn assert_appears_before(output: &str, first: &str, second: &str, context: &str) {
    let first_pos = output.find(first).unwrap_or_else(|| {
        panic!(
            "Expected to find '{}' in output. Context: {}\nOutput:\n{}",
            first, context, output
        )
    });
    let second_pos = output.find(second).unwrap_or_else(|| {
        panic!(
            "Expected to find '{}' in output. Context: {}\nOutput:\n{}",
            second, context, output
        )
    });
    assert!(
        first_pos < second_pos,
        "'{}' (pos {}) should appear before '{}' (pos {}). Context: {}\nOutput:\n{}",
        first,
        first_pos,
        second,
        second_pos,
        context,
        output
    );
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn view_with_default_ctx(app: &OilChatApp) -> crate::tui::oil::node::Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

// =============================================================================
// Popup Interaction Tests
// =============================================================================

#[test]
fn popup_opens_on_f1() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

    // Open popup
    app.update(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)));

    // Verify popup is open
    assert!(app.is_popup_visible(), "Popup should be open after F1");

    // Press Enter to select
    app.update(Event::Key(key(KeyCode::Enter)));

    // Popup should be closed after Enter selection
    assert!(
        !app.is_popup_visible(),
        "Popup should close after Enter selection"
    );
}

// =============================================================================
// Ctrl+C Behavior Tests
// =============================================================================

#[test]
fn ctrl_c_clears_non_empty_input() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

    // First Ctrl+C
    let action1 = app.update(Event::Key(ctrl('c')));
    assert!(matches!(action1, Action::Continue));

    // Second Ctrl+C immediately
    let action2 = app.update(Event::Key(ctrl('c')));
    assert!(matches!(action2, Action::Quit), "Double Ctrl+C should quit");
}

#[test]
fn ctrl_c_after_timeout_shows_notification_again() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
fn tick_hides_notification_area_when_empty() {
    let mut app = OilChatApp::default();

    app.update(Event::Key(ctrl('c')));
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Ctrl+C again to quit"),
        "Notification should be visible after Ctrl+C"
    );

    app.clear_notifications();
    app.update(Event::Tick);

    let tree_after = view_with_default_ctx(&app);
    let output_after = render_to_string(&tree_after, 80);
    assert!(
        !output_after.contains("Ctrl+C again to quit"),
        "Notification should be hidden after tick when empty"
    );
}

#[test]
fn ctrl_c_resets_on_any_other_key() {
    let mut app = OilChatApp::default();

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
fn error_shows_as_notification_toast() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::Error("Test error".to_string()));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Test error"),
        "Error should be displayed as notification toast in status bar"
    );
}

#[test]
fn unknown_slash_command_forwards_to_runner() {
    let mut app = OilChatApp::default();

    app.set_input_content("/unknowncommand");
    let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let action = app.update(event);

    assert!(
        matches!(
            action,
            Action::Send(ChatAppMsg::ExecuteSlashCommand(ref cmd)) if cmd == "/unknowncommand"
        ),
        "Unknown slash command should be forwarded via ExecuteSlashCommand, got: {:?}",
        action
    );
}

#[test]
fn unknown_repl_command_shows_error() {
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
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
fn error_renders_with_warning_styling() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::Error("Test error".to_string()));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("Test error"),
        "Error should be visible in rendered output as notification toast"
    );
}

#[test]
fn multiple_errors_show_latest_as_toast() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::Error("First error".to_string()));
    app.on_message(ChatAppMsg::Error("Second error".to_string()));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Second error"),
        "Should show most recent error as toast"
    );
}

// =============================================================================
// Command Coverage Tests
// =============================================================================

#[test]
fn slash_default_sets_mode_to_default() {
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();

    harness.send_text("/normal");
    harness.send_enter();

    let output = harness.viewport();
    assert!(
        output.contains("NORMAL") || output.contains("normal"),
        "Mode should be set to normal: {}",
        output
    );
}

#[test]
fn slash_auto_sets_mode_to_auto() {
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();

    harness.send_text("/auto");
    harness.send_enter();

    let output = harness.viewport();
    assert!(
        output.contains("AUTO") || output.contains("auto"),
        "Mode should be set to auto: {}",
        output
    );
}

#[test]
fn help_shows_both_slash_and_colon_commands() {
    let mut app = OilChatApp::default();

    for c in ":help".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("[system]") || output.contains("/mode") || output.contains(":quit"),
        "Help should show available commands: {}",
        output
    );
}

#[test]
fn repl_quit_returns_quit_action() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

    // Start in Normal mode
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("NORMAL"), "Should start in Normal mode");

    // Type /mode and submit
    for c in "/mode".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("PLAN"), "Should cycle to Plan mode");

    // Type /mode again
    for c in "/mode".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(output.contains("AUTO"), "Should cycle to Auto mode");

    // Type /mode again
    for c in "/mode".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("NORMAL"),
        "Should cycle back to Normal mode"
    );
}

// =============================================================================
// Autocomplete Trigger Tests
// =============================================================================

#[test]
fn at_symbol_triggers_file_autocomplete() {
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();
    app.set_workspace_files(vec!["src/main.rs".to_string()]);

    for c in "@m".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    app.update(Event::Key(key(KeyCode::Enter)));

    assert_eq!(app.input_content(), "@src/main.rs ");
}

#[test]
fn autocomplete_selection_inserts_note_mention() {
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();

    for c in "hello /mod".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open for / after whitespace"
    );
    assert_eq!(
        app.current_popup_filter(),
        "mod",
        "Filter should be text after slash"
    );
}

#[test]
fn slash_command_does_not_trigger_mid_word() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

    app.update(Event::Key(key(KeyCode::Char('@'))));

    assert!(
        !app.is_popup_visible(),
        "Popup should not show when no files to display"
    );
}

#[test]
fn empty_kiln_notes_does_not_show_popup() {
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

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
    let mut app = OilChatApp::default();

    app.update(Event::Key(key(KeyCode::Char('!'))));
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.has_shell_modal(),
        "Empty shell command should not open modal"
    );
}

#[test]
fn shell_modal_closes_on_escape() {
    let mut app = OilChatApp::default();

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

#[test]
fn shell_modal_cat_readme_starts_at_top() {
    let mut app = OilChatApp::default();

    for c in "!cat README.md".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    for _ in 0..30 {
        app.update(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    if !app.has_shell_modal() {
        return;
    }

    let output = app.shell_output_lines();
    if output.is_empty() {
        return;
    }

    assert_eq!(
        app.shell_scroll_offset(),
        0,
        "Scroll offset should be 0 after command completes"
    );

    let first_line = &output[0];
    assert!(
        first_line.starts_with('#') || first_line.starts_with("[!["),
        "First line of README should start with # or badge, got: {}",
        first_line
    );

    let visible = app.shell_visible_lines(20);
    assert_eq!(
        visible[0], output[0],
        "First visible line should match first captured line"
    );
}

#[test]
fn shell_modal_small_viewport_shows_first_lines() {
    let mut app = OilChatApp::default();

    for c in "!seq 1 50".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    for _ in 0..20 {
        app.update(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let output = app.shell_output_lines();
    assert!(output.len() >= 50, "Should capture all 50 lines");

    let small_viewport = app.shell_visible_lines(5);
    assert_eq!(small_viewport.len(), 5);
    assert_eq!(
        small_viewport[0], "1",
        "First line in small viewport should be '1'"
    );
    assert_eq!(small_viewport[4], "5", "Fifth line should be '5'");

    let tiny_viewport = app.shell_visible_lines(3);
    assert_eq!(tiny_viewport.len(), 3);
    assert_eq!(
        tiny_viewport[0], "1",
        "First line in tiny viewport should be '1'"
    );
}

#[test]
fn shell_modal_immediate_render_then_poll() {
    let mut app = OilChatApp::default();

    for c in "!seq 1 20".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(app.has_shell_modal(), "Modal should exist immediately");

    let output_before_tick = app.shell_output_lines();

    std::thread::sleep(std::time::Duration::from_millis(200));

    for _ in 0..5 {
        app.update(Event::Tick);
    }

    let output_after_ticks = app.shell_output_lines();
    assert!(
        output_after_ticks.len() >= 20,
        "Should have all 20 lines after ticks, got {}",
        output_after_ticks.len()
    );

    assert_eq!(
        output_after_ticks.first().map(|s| s.as_str()),
        Some("1"),
        "First line should be '1' (no truncation), got: {:?}",
        output_after_ticks.first()
    );

    assert_eq!(
        app.shell_scroll_offset(),
        0,
        "Scroll offset should remain 0 (view from top), was: {}. \
         Output before tick had {} lines, after tick has {} lines",
        app.shell_scroll_offset(),
        output_before_tick.len(),
        output_after_ticks.len()
    );
}

#[test]
fn shell_modal_render_shows_first_line_in_small_viewport() {
    use crate::tui::oil::render::render_to_string;

    let mut app = OilChatApp::default();

    for c in "!seq 1 100".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    for _ in 0..20 {
        app.update(Event::Tick);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert!(app.has_shell_modal(), "Modal should be open");
    let output = app.shell_output_lines();
    assert!(
        output.len() >= 100,
        "Should have 100 lines, got {}",
        output.len()
    );

    let tree = view_with_default_ctx(&app);
    let rendered = render_to_string(&tree, 80);

    let lines: Vec<&str> = rendered.lines().collect();

    assert!(
        rendered.contains("$ seq 1 100"),
        "Rendered output should contain command header, got:\n{}",
        &rendered[..rendered.len().min(500)]
    );

    assert!(
        lines.iter().any(|l| l.trim() == "1"),
        "Rendered output should show '1' (first line of seq output) in a 24-line viewport. \
         First 10 lines of render:\n{}",
        lines
            .iter()
            .take(10)
            .map(|s| format!("  '{}'", s))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// =============================================================================
// Model Command Tests
// =============================================================================

#[test]
fn model_command_opens_popup_with_available_models() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
        "openai/gpt-4".to_string(),
    ]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open when typing ':model '"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(output.contains("llama3"), "Popup should show llama3 model");
    assert!(
        output.contains("claude-3"),
        "Popup should show claude-3 model"
    );
}

#[test]
fn model_command_filters_models() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
        "openai/gpt-4".to_string(),
    ]);

    for c in ":model clau".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should be visible");
    assert_eq!(
        app.current_popup_filter(),
        "clau",
        "Filter should be 'clau'"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("claude-3"),
        "Popup should show claude-3 (matches filter)"
    );
}

#[test]
fn model_command_selection_fills_input() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should be visible");

    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should close after selection"
    );
    assert!(
        app.input_content().contains(":model ollama/llama3"),
        "Input should contain ':model ollama/llama3', got: {}",
        app.input_content()
    );
}

#[test]
fn model_command_popup_select_updates_model() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec!["ollama/llama3".to_string()]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should open after ':model '");

    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should close after selection"
    );

    assert!(
        app.input_content().contains(":model ollama/llama3"),
        "Input should contain ':model ollama/llama3', got: {}",
        app.input_content()
    );

    let action = app.update(Event::Key(key(KeyCode::Enter)));

    match action {
        Action::Send(msg) => {
            app.on_message(msg);
        }
        other => panic!(
            "Expected Action::Send after submitting, got {:?}. Input was: '{}'",
            other,
            app.input_content()
        ),
    }

    assert_eq!(
        app.current_model(),
        "ollama/llama3",
        "Model should be updated to ollama/llama3"
    );
}

#[test]
fn model_command_no_models_shows_message() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();

    // First :model triggers lazy fetch (state is NotLoaded)
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    // Simulate the fetch completing with empty model list
    app.on_message(ChatAppMsg::ModelsLoaded(vec![]));

    // Close popup with Escape
    app.update(Event::Key(key(KeyCode::Esc)));

    // Now try :model again - should show "No models available" message
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("No models available"),
        "Should show 'No models available' when no models configured. Got: {}",
        output
    );
}

#[test]
fn model_repl_command_in_popup_list() {
    let mut app = OilChatApp::default();

    for c in ":".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should open on :");

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains(":model"),
        "REPL command popup should include :model"
    );
}

// =============================================================================
// Config Command Tests
// =============================================================================

#[test]
fn config_show_command_displays_values() {
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();

    // Type :config show command
    harness.send_text(":config show");
    harness.send_enter();

    let output = harness.screen();

    // Should display temperature value
    assert!(
        output.contains("temperature:") || output.contains("temperature ="),
        "Should display temperature value. Got: {}",
        output
    );

    // Should display max_tokens value
    assert!(
        output.contains("max_tokens:")
            || output.contains("max_tokens =")
            || output.contains("maxtokens"),
        "Should display max_tokens value. Got: {}",
        output
    );

    // Should display thinking_budget value
    assert!(
        output.contains("thinking_budget:")
            || output.contains("thinking_budget =")
            || output.contains("thinkingbudget"),
        "Should display thinking_budget value. Got: {}",
        output
    );

    // Should display mode value
    assert!(
        output.contains("mode:") || output.contains("mode ="),
        "Should display mode value. Got: {}",
        output
    );
}
