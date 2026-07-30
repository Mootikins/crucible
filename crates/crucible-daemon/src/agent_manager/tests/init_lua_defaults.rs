//! The behaviour `defaults/init.lua` is responsible for, asserted against the
//! REAL daemon session VM (`get_or_create_session_state`) rather than a
//! hand-built `Lua`.
//!
//! Why that distinction matters: the previous default system prompt was
//! written as a `crucible.on_session_start` hook, but that API is registered
//! only by `LuaExecutor` — never on the daemon's session VM. The guard
//! `if type(crucible.on_session_start) == "function"` was therefore always
//! false and the prompt silently never applied, while `init_lua.rs`'s
//! "loads without error" test stayed green throughout. These tests assert the
//! effect, so a default that registers against a missing API fails loudly.

use super::*;
use crucible_lua::{execute_permission_hooks, PermissionHookResult, PermissionRequest};

async fn session_with_defaults() -> (TempDir, Arc<AgentManager>, String) {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let agent_manager = Arc::new(create_test_agent_manager(session_manager));
    let id = session.id.clone();
    (tmp, agent_manager, id)
}

/// Run the session VM's registered permission hooks the same way
/// `execute_permission_hooks_with_timeout` does.
async fn run_permission_hooks(
    agent_manager: &AgentManager,
    session_id: &str,
    request: PermissionRequest,
) -> PermissionHookResult {
    let state = agent_manager.get_or_create_session_state(session_id);
    let guard = state.lock().await;
    let hooks = guard.permission_hooks.lock().unwrap();
    let functions = guard.permission_functions.lock().unwrap();
    assert!(
        !hooks.is_empty(),
        "defaults/init.lua must register a permission hook on the session VM; \
         an empty list means crucible.permissions.on_request was missing there"
    );
    execute_permission_hooks(&guard.lua, &hooks, &functions, &request).unwrap()
}

fn tool_request(mode: &str) -> PermissionRequest {
    PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({ "command": "rm -rf build" }),
        file_path: None,
        mode: Some(mode.to_string()),
    }
}

#[tokio::test]
async fn auto_mode_approves_a_permission_request_without_prompting() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let result = run_permission_hooks(&agent_manager, &session_id, tool_request("auto")).await;

    assert_eq!(
        result,
        PermissionHookResult::Allow,
        "auto mode is documented as 'Auto-approve all operations'"
    );
}

#[test_case::test_case("normal"; "normal mode still prompts")]
#[test_case::test_case("plan"; "plan mode still prompts")]
#[tokio::test]
async fn non_auto_modes_still_reach_the_prompt(mode: &str) {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let result = run_permission_hooks(&agent_manager, &session_id, tool_request(mode)).await;

    assert_eq!(
        result,
        PermissionHookResult::Prompt,
        "only auto mode may skip the prompt"
    );
}

/// A hook that cannot see the mode is the failure this plumbing exists to
/// prevent: it would fall through to Prompt for every request, and auto mode
/// would look like it "just doesn't work".
#[tokio::test]
async fn request_without_a_mode_falls_through_to_the_prompt() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let mut request = tool_request("auto");
    request.mode = None;

    let result = run_permission_hooks(&agent_manager, &session_id, request).await;

    assert_eq!(result, PermissionHookResult::Prompt);
}

/// Drive the real `transform_context` dispatch so the assertion covers the
/// payload the daemon actually sends, not a hand-rolled event.
async fn run_transform_context(
    agent_manager: &AgentManager,
    session_id: &str,
    system_prompt: &str,
    messages: Vec<crucible_core::traits::ContextMessage>,
) -> Vec<crucible_core::traits::ContextMessage> {
    let state = agent_manager.get_or_create_session_state(session_id);
    let guard = state.lock().await;
    let mut current = messages;

    for handler in guard
        .registry
        .runtime_handlers_for("transform_context", None)
    {
        let event = SessionEvent::Custom {
            name: "transform_context".to_string(),
            payload: serde_json::json!({
                "messages": &current,
                "model": "test-model",
                "system_prompt": system_prompt,
            }),
        };
        if let Ok(crucible_lua::ScriptHandlerResult::Transform(val)) = guard
            .registry
            .execute_runtime_handler(&guard.lua, &handler.name, &event, Some(session_id))
            .await
        {
            if let Some(msgs) = val.get("messages") {
                current = serde_json::from_value(msgs.clone()).expect("handler returned messages");
            }
        }
    }
    current
}

#[tokio::test]
async fn a_session_without_an_agent_prompt_gets_the_default_system_prompt() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let out = run_transform_context(
        &agent_manager,
        &session_id,
        "",
        vec![crucible_core::traits::ContextMessage::user("hello")],
    )
    .await;

    assert_eq!(
        out.first().map(|m| m.role),
        Some(crucible_core::traits::llm::MessageRole::System),
        "the default prompt must be prepended, not appended"
    );
    assert!(
        out[0].content.contains("Crucible"),
        "expected the built-in default prompt, got: {}",
        out[0].content
    );
    assert_eq!(
        out.len(),
        2,
        "the original conversation must be preserved beneath the injected prompt"
    );
    assert_eq!(out[1].content, "hello");
}

/// An agent card's prompt reaches the provider through a separate field, so
/// it is invisible in `messages`. Without the `system_prompt` payload field
/// the default would stack a second, conflicting instruction set on top of
/// every card-configured agent.
#[tokio::test]
async fn an_agent_card_prompt_suppresses_the_default() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let out = run_transform_context(
        &agent_manager,
        &session_id,
        "You are a haiku bot.",
        vec![crucible_core::traits::ContextMessage::user("hello")],
    )
    .await;

    assert_eq!(
        out.len(),
        1,
        "an agent with its own system prompt must not also get the default"
    );
    assert_eq!(out[0].content, "hello");
}

#[tokio::test]
async fn an_existing_system_message_suppresses_the_default() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let out = run_transform_context(
        &agent_manager,
        &session_id,
        "",
        vec![
            crucible_core::traits::ContextMessage::system("Pre-existing instructions."),
            crucible_core::traits::ContextMessage::user("hello"),
        ],
    )
    .await;

    assert_eq!(out.len(), 2, "must not prepend a second system message");
    assert_eq!(out[0].content, "Pre-existing instructions.");
}
