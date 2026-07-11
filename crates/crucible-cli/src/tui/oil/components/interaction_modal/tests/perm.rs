use super::key_event;
use crate::tui::oil::components::interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, InteractionMode,
};
use crossterm::event::KeyCode;
use crucible_core::interaction::{InteractionRequest, PermRequest};
use crucible_core::types::acp::FileDiff;
use crucible_oil::render::render_to_string;
use std::collections::HashSet;

#[test]
fn test_perm_modal_allow() {
    let perm = PermRequest::bash(["npm", "install"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('y'))));
    match output {
        InteractionModalOutput::PermissionResponse {
            request_id,
            response,
        } => {
            assert_eq!(request_id, "req-1");
            assert!(response.allowed);
        }
        _ => panic!("Expected PermissionResponse"),
    }
}

#[test]
fn test_perm_modal_deny() {
    let perm = PermRequest::bash(["npm", "install"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('n'))));
    match output {
        InteractionModalOutput::PermissionResponse {
            request_id,
            response,
        } => {
            assert_eq!(request_id, "req-1");
            assert!(!response.allowed);
        }
        _ => panic!("Expected PermissionResponse"),
    }
}

#[test]
fn test_perm_modal_navigation() {
    let perm = PermRequest::bash(["npm", "install"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    assert_eq!(modal.selected, 0);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 1);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Up)));
    assert_eq!(modal.selected, 0);
}

#[test]
fn test_wrap_selection() {
    assert_eq!(InteractionModal::wrap_selection(0, -1, 3), 2);
    assert_eq!(InteractionModal::wrap_selection(2, 1, 3), 0);
    assert_eq!(InteractionModal::wrap_selection(1, -1, 3), 0);
    assert_eq!(InteractionModal::wrap_selection(1, 1, 3), 2);
}

#[test]
fn test_toggle_checked() {
    let mut set = HashSet::new();
    InteractionModal::toggle_checked(&mut set, 1);
    assert!(set.contains(&1));
    InteractionModal::toggle_checked(&mut set, 1);
    assert!(!set.contains(&1));
}

#[test]
fn test_perm_modal_allowlist_shortcut() {
    let perm = PermRequest::bash(["cargo", "build"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('a'))));
    match output {
        InteractionModalOutput::PermissionResponse { response, .. } => {
            assert!(response.allowed);
            assert!(response.pattern.is_some());
            assert_eq!(response.pattern.unwrap(), "cargo *");
        }
        _ => panic!("Expected PermissionResponse with pattern"),
    }
}

#[test]
fn test_perm_modal_tab_opens_text_input() {
    let perm = PermRequest::bash(["npm", "install"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    assert_eq!(modal.mode, InteractionMode::Selecting);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
    assert_eq!(modal.mode, InteractionMode::TextInput);
}

#[test]
fn test_perm_modal_tab_on_allowlist_prefills_pattern() {
    let perm = PermRequest::bash(["cargo", "test"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 2);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
    assert_eq!(modal.mode, InteractionMode::TextInput);
    assert_eq!(modal.other_text, "cargo *");
}

#[test]
fn test_perm_modal_deny_with_reason() {
    let perm = PermRequest::bash(["rm", "-rf", "/"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 1);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
    assert_eq!(modal.mode, InteractionMode::TextInput);

    for c in "too dangerous".chars() {
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
    }

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    match output {
        InteractionModalOutput::PermissionResponse { response, .. } => {
            assert!(!response.allowed);
            assert_eq!(response.reason.as_deref(), Some("too dangerous"));
        }
        _ => panic!("Expected PermissionResponse with reason"),
    }
}

#[test]
fn test_perm_modal_esc_from_text_returns_to_selecting() {
    let perm = PermRequest::bash(["npm", "install"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Tab)));
    assert_eq!(modal.mode, InteractionMode::TextInput);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
    assert_eq!(modal.mode, InteractionMode::Selecting);
}

#[test]
fn snap_perm_popup_with_edit_diff() {
    let req =
        PermRequest::tool("edit", serde_json::json!({"file_path": "src/foo.rs"})).with_diffs(vec![
            FileDiff::from_contents("src/foo.rs", Some("fn old() {}\n".into()), "fn new() {}\n"),
        ]);
    let modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(req),
        true,
    );
    let node = modal.view(120, 0);
    let out = render_to_string(&node, 120);
    insta::assert_snapshot!(out);
}

#[test]
fn perm_prompt_shows_full_long_bash_command() {
    let long_arg = "very-long-argument-".repeat(12);
    let perm = PermRequest::bash(vec![
        "cargo".to_string(),
        "run".to_string(),
        "--example".to_string(),
        long_arg,
        "--final-flag".to_string(),
    ]);
    let modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );
    let out = render_to_string(&modal.view(80, 0), 80);
    assert!(
        out.contains("--final-flag"),
        "tail of a long command must be visible, not truncated:\n{out}"
    );
}

#[test]
fn perm_prompt_shows_full_tool_args() {
    // 4+ keys and a >30-char string previously fell to the `(k=v, ...)`
    // compact form; full mode must show every key and the whole value.
    let args = serde_json::json!({
        "alpha": 1,
        "beta": 2,
        "gamma": 3,
        "z_payload": format!("{}SENTINEL_END", "x".repeat(60)),
    });
    let perm = PermRequest::tool("mcp_call", args);
    let modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );
    let out = render_to_string(&modal.view(120, 0), 120);
    assert!(out.contains("z_payload"), "all arg keys visible:\n{out}");
    assert!(out.contains("SENTINEL_END"), "full value visible:\n{out}");
    assert!(!out.contains("..."), "no truncation marker:\n{out}");
}

#[test]
fn perm_prompt_compact_mode_truncates_to_one_line() {
    let long_arg = "very-long-argument-".repeat(12);
    let perm = PermRequest::bash(vec![
        "cargo".to_string(),
        "run".to_string(),
        long_arg,
        "--final-flag".to_string(),
    ]);
    let modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    )
    .with_full_commands(false);
    let out = render_to_string(&modal.view(80, 0), 80);
    assert!(
        !out.contains("--final-flag"),
        "compact mode keeps the old one-line view:\n{out}"
    );
    assert!(out.contains('…'), "compact mode ellipsizes:\n{out}");
}

#[test]
fn snap_perm_popup_wrapped_bash_command() {
    let perm = PermRequest::bash(vec![
        "cargo".to_string(),
        "nextest".to_string(),
        "run".to_string(),
        "--profile".to_string(),
        "integration".to_string(),
        "-E".to_string(),
        "test(interaction_modal) & package(crucible-cli)".to_string(),
        "--no-capture".to_string(),
    ]);
    let modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );
    let node = modal.view(60, 0);
    let out = render_to_string(&node, 60);
    insta::assert_snapshot!(out);
}

#[test]
fn test_perm_modal_navigation_wraps_at_3() {
    let perm = PermRequest::bash(["npm", "install"]);
    let mut modal = InteractionModal::new(
        "req-1".to_string(),
        InteractionRequest::Permission(perm),
        true,
    );

    assert_eq!(modal.selected, 0);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Up)));
    assert_eq!(modal.selected, 2);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.selected, 0);
}
