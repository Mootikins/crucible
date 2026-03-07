use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn snapshot_popup_hidden_baseline() {
    let mut app = OilChatApp::default();
    app.set_workspace_files(vec![
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "Cargo.toml".to_string(),
    ]);
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_popup_visible_same_height() {
    let mut app = OilChatApp::default();
    app.set_workspace_files(vec![
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "Cargo.toml".to_string(),
    ]);

    app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('@'))));

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_input_empty() {
    let app = OilChatApp::default();
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_input_short_text() {
    let mut app = OilChatApp::default();
    for c in "Hello".chars() {
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
    }
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_input_long_text_clamped() {
    let mut app = OilChatApp::default();
    let long_text = "x".repeat(300);
    for c in long_text.chars() {
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
    }
    assert_snapshot!(render_app(&app));
}

#[test]
fn verify_input_height_grows_with_content() {
    use crate::tui::oil::chat_app::INPUT_MAX_CONTENT_LINES;

    let app_empty = OilChatApp::default();

    let mut app_long = OilChatApp::default();
    for c in "x".repeat(300).chars() {
        app_long.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
    }

    let empty_output = render_app(&app_empty);
    let long_output = render_app(&app_long);

    let empty_lines = empty_output.lines().count();
    let long_lines = long_output.lines().count();

    assert!(
        empty_lines < long_lines,
        "Long input ({} lines) should have more lines than empty ({} lines)",
        long_lines,
        empty_lines
    );

    let max_input_height = INPUT_MAX_CONTENT_LINES + 2;
    assert!(
        long_lines <= empty_lines + max_input_height,
        "Long input growth should be bounded by max content lines"
    );
}
