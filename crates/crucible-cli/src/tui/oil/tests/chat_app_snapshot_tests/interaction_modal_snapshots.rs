use super::*;
use crucible_core::interaction::{AskRequest, InteractionRequest, PermRequest};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Snapshot test: AskRequest with 3 choices, first selected (default)
#[test]
fn snapshot_ask_modal_with_choices_first_selected() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Which programming language do you prefer?").choices([
            "Rust",
            "Python",
            "TypeScript",
        ]),
    );
    app.open_interaction("ask-1".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: AskRequest with 3 choices, second selected
#[test]
fn snapshot_ask_modal_with_choices_second_selected() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Which programming language do you prefer?").choices([
            "Rust",
            "Python",
            "TypeScript",
        ]),
    );
    app.open_interaction("ask-2".to_string(), request);

    // Navigate down to select second option
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: AskRequest with allow_other showing "Other..." option
#[test]
fn snapshot_ask_modal_with_allow_other() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Select your favorite or enter custom:")
            .choices(["Option A", "Option B"])
            .allow_other(),
    );
    app.open_interaction("ask-3".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: AskRequest free-text only (no choices)
#[test]
fn snapshot_ask_modal_free_text_only() {
    let mut app = OilChatApp::default();
    let request =
        InteractionRequest::Ask(AskRequest::new("Enter your custom value:").allow_other());
    app.open_interaction("ask-4".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: PermRequest for bash command
#[test]
fn snapshot_perm_modal_bash_command() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["npm", "install", "lodash"]));
    app.open_interaction("perm-bash".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: PermRequest for file write
#[test]
fn snapshot_perm_modal_file_write() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::write([
        "home", "user", "project", "src", "main.rs",
    ]));
    app.open_interaction("perm-write".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: PermRequest for file read
#[test]
fn snapshot_perm_modal_file_read() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::read(["etc", "hosts"]));
    app.open_interaction("perm-read".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: PermRequest for tool execution
#[test]
fn snapshot_perm_modal_tool() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::tool(
        "semantic_search",
        serde_json::json!({"query": "rust memory safety", "limit": 10}),
    ));
    app.open_interaction("perm-tool".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: AskRequest with many choices (scrolling)
#[test]
fn snapshot_ask_modal_many_choices() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(AskRequest::new("Select an option:").choices([
        "First option",
        "Second option",
        "Third option",
        "Fourth option",
        "Fifth option",
        "Sixth option",
        "Seventh option",
        "Eighth option",
    ]));
    app.open_interaction("ask-many".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

/// Snapshot test: AskRequest after navigating to last choice
#[test]
fn snapshot_ask_modal_last_selected() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Pick one:").choices(["Alpha", "Beta", "Gamma", "Delta"]),
    );
    app.open_interaction("ask-last".to_string(), request);

    // Navigate to last option
    for _ in 0..3 {
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
    }

    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Multi-select Mode Tests
// =========================================================================

#[test]
fn snapshot_ask_modal_multi_select() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Select all languages you know:")
            .choices(["Rust", "Python", "Go", "TypeScript"])
            .multi_select(),
    );
    app.open_interaction("ask-multi".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_ask_modal_multi_select_with_selection() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Select frameworks:")
            .choices(["React", "Vue", "Angular", "Svelte"])
            .multi_select(),
    );
    app.open_interaction("ask-multi-sel".to_string(), request);

    // Toggle first item with Space
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
    )));
    // Move down and toggle second
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Completion Flow Tests
// =========================================================================

#[test]
fn snapshot_ask_modal_after_escape() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(AskRequest::new("Choose:").choices(["Yes", "No"]));
    app.open_interaction("ask-esc".to_string(), request);

    // Verify modal is visible
    assert!(app.interaction_visible());

    // Press Escape to cancel
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Esc)));

    // Modal should be closed - snapshot shows regular view
    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_ask_modal_after_ctrl_c() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(AskRequest::new("Choose:").choices(["Yes", "No"]));
    app.open_interaction("ask-ctrl-c".to_string(), request);

    // Verify modal is visible
    assert!(app.interaction_visible());

    // Press Ctrl+C to cancel
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));

    // Modal should be closed - snapshot shows regular view
    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_perm_modal_after_allow() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
    app.open_interaction("perm-allow".to_string(), request);

    // Press 'y' to allow
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('y'),
        KeyModifiers::NONE,
    )));

    // Modal should be closed
    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_perm_modal_after_deny() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/"]));
    app.open_interaction("perm-deny".to_string(), request);

    // Press 'n' to deny
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
    )));

    // Modal should be closed
    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Edge Case Tests
// =========================================================================

#[test]
fn snapshot_ask_modal_long_question_text() {
    let mut app = OilChatApp::default();
    let long_question = "This is a very long question that should test how the modal handles text overflow. It contains multiple sentences to ensure we're testing a realistic scenario where the agent asks a detailed question that might wrap across multiple lines in the terminal.";
    let request = InteractionRequest::Ask(
        AskRequest::new(long_question).choices(["Accept", "Reject", "Skip"]),
    );
    app.open_interaction("ask-long".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_ask_modal_long_choice_text() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Select option:").choices([
            "Short",
            "This is a much longer choice that might need to be truncated or wrapped depending on the terminal width",
            "Medium length option here",
        ]),
    );
    app.open_interaction("ask-long-choice".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_ask_modal_unicode_content() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Ask(
        AskRequest::new("Select your preferred emoji reaction:").choices([
            "👍 Thumbs up",
            "❤️ Heart",
            "🎉 Party",
            "🚀 Rocket",
            "🤔 Thinking",
        ]),
    );
    app.open_interaction("ask-unicode".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_perm_modal_long_command() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::bash([
        "docker",
        "run",
        "--rm",
        "-it",
        "-v",
        "/home/user/project:/app",
        "-e",
        "DATABASE_URL=postgres://localhost/db",
        "-p",
        "8080:8080",
        "myimage:latest",
    ]));
    app.open_interaction("perm-long-cmd".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_perm_modal_deeply_nested_path() {
    let mut app = OilChatApp::default();
    let request = InteractionRequest::Permission(PermRequest::write([
        "home",
        "user",
        "projects",
        "company",
        "team",
        "repository",
        "packages",
        "core",
        "src",
        "components",
        "Button.tsx",
    ]));
    app.open_interaction("perm-deep-path".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Show Interaction Tests
// =========================================================================

#[test]
fn snapshot_show_modal_basic() {
    use crucible_core::interaction::ShowRequest;

    let mut app = OilChatApp::default();
    let content = "This is some content to display.\nIt has multiple lines.\nLine three here.\nAnd a fourth line for good measure.";
    let request = InteractionRequest::Show(ShowRequest::new(content).title("Preview"));
    app.open_interaction("show-1".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_show_modal_after_scroll() {
    use crucible_core::interaction::ShowRequest;

    let mut app = OilChatApp::default();
    let lines: Vec<String> = (1..=30).map(|i| format!("Line number {i}")).collect();
    let content = lines.join("\n");
    let request = InteractionRequest::Show(ShowRequest::new(&content));
    app.open_interaction("show-scroll".to_string(), request);

    for _ in 0..5 {
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('j'))));
    }

    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Popup Interaction Tests
// =========================================================================

#[test]
fn snapshot_popup_modal_basic() {
    use crucible_core::interaction::PopupRequest;
    use crucible_core::types::PopupEntry;

    let mut app = OilChatApp::default();
    let request = InteractionRequest::Popup(
        PopupRequest::new("Select an action")
            .entry(PopupEntry::new("Open").with_description("Open the file in editor"))
            .entry(PopupEntry::new("Delete").with_description("Remove permanently"))
            .entry(PopupEntry::new("Rename")),
    );
    app.open_interaction("popup-1".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_popup_modal_second_selected() {
    use crucible_core::interaction::PopupRequest;
    use crucible_core::types::PopupEntry;

    let mut app = OilChatApp::default();
    let request = InteractionRequest::Popup(
        PopupRequest::new("Pick one")
            .entry(PopupEntry::new("Alpha"))
            .entry(PopupEntry::new("Beta"))
            .entry(PopupEntry::new("Gamma")),
    );
    app.open_interaction("popup-nav".to_string(), request);

    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_popup_modal_with_allow_other() {
    use crucible_core::interaction::PopupRequest;
    use crucible_core::types::PopupEntry;

    let mut app = OilChatApp::default();
    let request = InteractionRequest::Popup(
        PopupRequest::new("Choose or type custom")
            .entry(PopupEntry::new("Option A"))
            .entry(PopupEntry::new("Option B"))
            .allow_other(),
    );
    app.open_interaction("popup-other".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Edit Interaction Tests
// =========================================================================

#[test]
fn snapshot_edit_modal_normal_mode() {
    use crucible_core::interaction::EditRequest;

    let mut app = OilChatApp::default();
    let content = "fn main() {\n    println!(\"Hello\");\n}";
    let request = InteractionRequest::Edit(EditRequest::new(content).hint("Edit the function"));
    app.open_interaction("edit-1".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_edit_modal_insert_mode() {
    use crucible_core::interaction::EditRequest;

    let mut app = OilChatApp::default();
    let content = "first line\nsecond line\nthird line";
    let request = InteractionRequest::Edit(EditRequest::new(content));
    app.open_interaction("edit-insert".to_string(), request);

    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('i'))));

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_edit_modal_cursor_moved() {
    use crucible_core::interaction::EditRequest;

    let mut app = OilChatApp::default();
    let content = "line one\nline two\nline three";
    let request = InteractionRequest::Edit(EditRequest::new(content));
    app.open_interaction("edit-cursor".to_string(), request);

    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('j'))));
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('l'))));
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('l'))));

    assert_snapshot!(render_app(&mut app));
}

// =========================================================================
// Panel Interaction Tests
// =========================================================================

#[test]
fn snapshot_panel_modal_basic() {
    use crucible_core::interaction::{InteractivePanel, PanelItem};

    let mut app = OilChatApp::default();
    let request =
        InteractionRequest::Panel(InteractivePanel::new("Select files to process").items([
            PanelItem::new("README.md").with_description("Project readme"),
            PanelItem::new("Cargo.toml").with_description("Rust manifest"),
            PanelItem::new("src/main.rs"),
            PanelItem::new("src/lib.rs"),
        ]));
    app.open_interaction("panel-1".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_panel_modal_multi_select_with_checks() {
    use crucible_core::interaction::{InteractivePanel, PanelHints, PanelItem};

    let mut app = OilChatApp::default();
    let request = InteractionRequest::Panel(
        InteractivePanel::new("Toggle items")
            .items([
                PanelItem::new("Apple"),
                PanelItem::new("Banana"),
                PanelItem::new("Cherry"),
                PanelItem::new("Date"),
            ])
            .hints(PanelHints::new().multi_select()),
    );
    app.open_interaction("panel-multi".to_string(), request);

    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
    )));
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_panel_modal_filterable() {
    use crucible_core::interaction::{InteractivePanel, PanelHints, PanelItem};

    let mut app = OilChatApp::default();
    let request = InteractionRequest::Panel(
        InteractivePanel::new("Search and select")
            .items([
                PanelItem::new("Apple"),
                PanelItem::new("Apricot"),
                PanelItem::new("Banana"),
                PanelItem::new("Blueberry"),
                PanelItem::new("Cherry"),
            ])
            .hints(PanelHints::new().filterable()),
    );
    app.open_interaction("panel-filter".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}

#[test]
fn snapshot_panel_modal_with_initial_selection() {
    use crucible_core::interaction::{InteractivePanel, PanelHints, PanelItem};

    let mut app = OilChatApp::default();
    let request = InteractionRequest::Panel(
        InteractivePanel::new("Pre-selected items")
            .items([
                PanelItem::new("Item A"),
                PanelItem::new("Item B"),
                PanelItem::new("Item C"),
                PanelItem::new("Item D"),
            ])
            .hints(PanelHints::new().multi_select().initial_selection([0, 2])),
    );
    app.open_interaction("panel-presel".to_string(), request);

    assert_snapshot!(render_app(&mut app));
}
