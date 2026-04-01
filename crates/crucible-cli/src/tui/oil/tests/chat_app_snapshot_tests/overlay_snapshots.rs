use super::*;
use crucible_core::types::Notification;

fn default_statusline_config() -> crucible_lua::statusline::StatuslineConfig {
    crucible_lua::statusline::StatuslineConfig::builtin_default()
}

/// Scenario 4: :messages drawer with notification history
#[test]
fn snapshot_messages_drawer_with_history() {
    let mut app = OilChatApp::default();

    app.add_notification(Notification::toast("Session saved"));
    app.add_notification(Notification::toast("Thinking display: on"));
    app.add_notification(Notification::progress(45, 100, "Indexing files"));
    app.add_notification(Notification::warning("Context at 85%"));

    app.show_messages();

    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "[TIME]");
    settings.bind(|| {
        assert_snapshot!(render_app(&mut app));
    });
}

/// :messages command opens drawer during streaming
#[test]
fn snapshot_messages_drawer_during_streaming() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Hi there!".to_string()));

    app.add_notification(Notification::toast("Session saved"));
    app.add_notification(Notification::warning("Context at 85%"));

    app.show_messages();
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "[TIME]");
    settings.bind(|| {
        assert_snapshot!(render_app(&mut app));
    });
}

/// Scenario 7: Recent warnings show as toast; counts show after expiry
#[test]
fn snapshot_statusline_warning_counts() {
    use crate::tui::oil::components::status_bar::NotificationToastKind;
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(crate::tui::oil::chat_app::ChatMode::Normal)
        .model("gpt-4o")
        .counts(vec![
            (NotificationToastKind::Warning, 3),
            (NotificationToastKind::Error, 1),
        ]);
    let node = bar.view_from_config(&default_statusline_config());
    let output = crate::tui::oil::render::render_to_plain_text(&node, 80);
    assert_snapshot!(output);
}

/// Scenario 1: Simple info toast on statusline (drawer closed)
#[test]
fn snapshot_statusline_info_toast() {
    let mut app = OilChatApp::default();
    app.add_notification(Notification::toast("Session saved"));
    app.hide_messages();
    assert_snapshot!(render_app(&mut app));
}

/// Scenario 3: Warning toast on statusline (drawer closed)
#[test]
fn snapshot_statusline_warning_toast() {
    let mut app = OilChatApp::default();
    app.add_notification(Notification::warning("Context at 85%"));
    app.hide_messages();
    assert_snapshot!(render_app(&mut app));
}

/// Drawer with conversation content above
#[test]
fn snapshot_messages_drawer_with_conversation() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Hi there!".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    app.add_notification(Notification::toast("Session saved"));
    app.add_notification(Notification::warning("Context at 85%"));

    app.show_messages();
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "[TIME]");
    settings.bind(|| {
        assert_snapshot!(render_app(&mut app));
    });
}

/// Empty drawer (no notifications)
#[test]
fn snapshot_messages_drawer_empty() {
    let mut app = OilChatApp::default();
    app.show_messages();
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "[TIME]");
    settings.bind(|| {
        assert_snapshot!(render_app(&mut app));
    });
}

#[test]
fn snapshot_raw_drawer_with_history() {
    let mut app = OilChatApp::default();
    app.add_notification(Notification::toast("Session saved"));
    app.add_notification(Notification::toast("Thinking display: on"));
    app.add_notification(Notification::progress(45, 100, "Indexing files"));
    app.add_notification(Notification::warning("Context at 85%"));
    app.show_messages();
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d{2}:\d{2}:\d{2}", "[TIME]");
    settings.bind(|| {
        assert_snapshot!(render_app_raw(&mut app));
    });
}

#[test]
fn snapshot_raw_statusline_info_toast() {
    let mut app = OilChatApp::default();
    app.add_notification(Notification::toast("Session saved"));
    app.hide_messages();
    assert_snapshot!(render_app_raw(&mut app));
}

#[test]
fn snapshot_raw_statusline_warning_toast() {
    let mut app = OilChatApp::default();
    app.add_notification(Notification::warning("Context at 85%"));
    app.hide_messages();
    assert_snapshot!(render_app_raw(&mut app));
}

#[test]
fn snapshot_raw_statusline_warning_counts() {
    use crate::tui::oil::components::status_bar::NotificationToastKind;
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(crate::tui::oil::chat_app::ChatMode::Normal)
        .model("gpt-4o")
        .counts(vec![
            (NotificationToastKind::Warning, 3),
            (NotificationToastKind::Error, 1),
        ]);
    let node = bar.view_from_config(&default_statusline_config());
    let output = crate::tui::oil::render::render_to_string(&node, 80);
    assert_snapshot!(output);
}

#[test]
fn snapshot_raw_perm_bash() {
    let mut app = OilChatApp::default();
    let request = crucible_core::interaction::InteractionRequest::Permission(
        crucible_core::interaction::PermRequest::bash(["npm", "install", "lodash"]),
    );
    app.open_interaction("perm-bash".to_string(), request);
    assert_snapshot!(render_app_raw(&mut app));
}

#[test]
fn snapshot_tool_call_visible_under_permission_prompt() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Run the build".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "I'll run the build for you.".to_string(),
    ));
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"cargo build"}"#.to_string(),
        call_id: None,
        description: None,
        source: None,
        lua_primary_arg: None,
    });

    let request = crucible_core::interaction::InteractionRequest::Permission(
        crucible_core::interaction::PermRequest::bash(["cargo", "build"]),
    );
    app.open_interaction("perm-build".to_string(), request);

    let output = render_app(&mut app);
    assert!(
        output.contains("cargo build"),
        "Tool call should be visible under permission prompt"
    );
    assert!(
        output.contains("PERMISSION"),
        "Permission modal should be visible"
    );
    assert_snapshot!(output);
}
