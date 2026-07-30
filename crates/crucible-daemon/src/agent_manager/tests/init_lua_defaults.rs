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
         an empty list means cru.permissions.on_request was missing there"
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

/// `configure_agent` is where a default becomes real, so the assertions run
/// through it rather than through the Lua store — a value that reaches
/// `cru.defaults` but never reaches `AgentConfig` is exactly the failure
/// the previous `transform_context` approach had.
async fn configured_agent(
    agent_manager: &AgentManager,
    session_manager: &SessionManager,
    session_id: &str,
    agent: SessionAgent,
) -> SessionAgent {
    agent_manager
        .configure_agent(session_id, agent)
        .await
        .expect("configure_agent must succeed");
    session_manager
        .get_session(session_id)
        .unwrap()
        .agent
        .expect("session must have an agent")
}

fn bare_agent() -> SessionAgent {
    let mut agent = test_agent();
    agent.system_prompt = String::new();
    agent.temperature = None;
    agent
}

#[tokio::test]
async fn an_agent_with_no_prompt_of_its_own_gets_the_default() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;
    let session_manager = agent_manager.session_manager.clone();

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    assert!(
        agent.system_prompt.contains("Crucible"),
        "expected the built-in default prompt, got: {:?}",
        agent.system_prompt
    );
}

/// The point of routing through `AgentConfig` rather than injecting a message
/// per turn: the value is session state, so every surface that reports a
/// system prompt (TUI `GetSystemPrompt`, Lua `session.system_prompt`, web)
/// sees it. Reading it back through the Lua session API is the closest
/// in-process proxy for "the UIs can see it".
#[tokio::test]
async fn the_default_is_visible_as_session_state_not_just_at_send_time() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;
    let session_manager = agent_manager.session_manager.clone();

    configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    let persisted = session_manager
        .get_session(&session_id)
        .unwrap()
        .agent
        .unwrap()
        .system_prompt;
    assert!(
        !persisted.is_empty(),
        "the default must be persisted on the session's agent config, not applied per turn"
    );
}

/// Defaults fill, they never override — that is what makes them defaults.
#[tokio::test]
async fn an_agent_card_prompt_wins_over_the_default() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;
    let session_manager = agent_manager.session_manager.clone();

    let mut agent = bare_agent();
    agent.system_prompt = "You are a haiku bot.".to_string();

    let configured = configured_agent(&agent_manager, &session_manager, &session_id, agent).await;

    assert_eq!(configured.system_prompt, "You are a haiku bot.");
}

/// `cru.defaults.x = …` is ordinary assignment on an ordinary VM, so a
/// later file overrides an earlier one with no special mechanism. This is the
/// whole extensibility story for defaults — if it fails, users cannot change
/// a shipped default without editing the shipped file.
#[tokio::test]
async fn a_user_init_lua_can_replace_a_shipped_default() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.defaults.system_prompt = "Only haiku.""#,
    )
    .unwrap();

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
    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));

    let agent = configured_agent(&agent_manager, &session_manager, &session.id, bare_agent()).await;

    assert_eq!(
        agent.system_prompt, "Only haiku.",
        "user init.lua runs after the built-in defaults, so its assignment must win"
    );
}

/// The append idiom, end to end: a user file extends the shipped prompt
/// instead of replacing it, using plain string concatenation.
#[tokio::test]
async fn a_user_init_lua_can_append_to_a_shipped_default() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.defaults.system_prompt =
             cru.defaults.system_prompt .. "\n\nAnswer in British English.""#,
    )
    .unwrap();

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
    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));

    let agent = configured_agent(&agent_manager, &session_manager, &session.id, bare_agent()).await;

    assert!(
        agent.system_prompt.contains("Crucible"),
        "the shipped prompt must survive the append"
    );
    assert!(
        agent.system_prompt.ends_with("Answer in British English."),
        "the user's addition must be appended, got: {:?}",
        agent.system_prompt
    );
}
