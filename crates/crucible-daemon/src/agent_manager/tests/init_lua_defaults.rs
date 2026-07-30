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
use crucible_core::config::components::permissions::PermissionDecision;
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
        is_safe: false,
    }
}

/// Auto's approval is a mode STANCE now, not a hook, so no hook answers for
/// it — the gate applies the stance after the hooks decline. The stance itself
/// is asserted in `the_auto_mode_stance_is_allow_and_plan_is_deny`.
#[tokio::test]
async fn auto_mode_registers_no_permission_hook() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let result = run_permission_hooks(&agent_manager, &session_id, tool_request("auto")).await;

    assert_eq!(result, PermissionHookResult::Prompt);
}

#[tokio::test]
async fn normal_mode_still_reaches_the_prompt() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let result = run_permission_hooks(&agent_manager, &session_id, tool_request("normal")).await;

    assert_eq!(
        result,
        PermissionHookResult::Prompt,
        "normal mode decides nothing on the user's behalf"
    );
}

/// Plan mode's rule is now STATED in Lua next to auto mode's, rather than
/// being implicit in which tools the daemon happens to advertise. The Rust
/// floor (tool-set filtering, plugin-tool dispatch ban) still enforces it
/// independently — this makes it legible and extensible, not load-bearing.
#[tokio::test]
async fn plan_mode_denies_a_mutating_tool() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let result = run_permission_hooks(&agent_manager, &session_id, tool_request("plan")).await;

    assert_eq!(result, PermissionHookResult::Deny);
}

/// An agent card's `ask` policy can push a read-only tool through the gate.
/// Denying it in plan mode would be wrong — plan mode forbids mutation, not
/// reading — which is why the request carries the daemon's own `is_safe`
/// classification instead of the hook assuming.
#[tokio::test]
async fn plan_mode_does_not_deny_a_read_only_tool() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let mut request = tool_request("plan");
    request.tool_name = "read_file".to_string();
    request.is_safe = true;

    let result = run_permission_hooks(&agent_manager, &session_id, request).await;

    assert_eq!(
        result,
        PermissionHookResult::Prompt,
        "plan mode forbids mutation, not reading"
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

/// `cru.on_session_start` was advertised by the shipped defaults, by
/// `cru setup`'s user template, and by the plugin docs — while being nil on
/// this VM, so every hook registered against it silently never ran.
#[tokio::test]
async fn on_session_start_fires_and_can_set_this_sessions_values() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.on_session_start(function(session)
             session.system_prompt = "per-session prompt"
             session.temperature = 0.25
           end)"#,
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

    assert_eq!(agent.system_prompt, "per-session prompt");
    assert_eq!(agent.temperature, Some(0.25));
}

/// The hook reads the INHERITED value before overriding it — the Neovim
/// pattern where a `FileType` autocmd sees the global option and sets the
/// buffer-local one. Without seeding, `session.system_prompt` would be nil
/// inside the hook and appending would error.
#[tokio::test]
async fn on_session_start_sees_the_global_default_and_can_extend_it() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.on_session_start(function(session)
             session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
           end)"#,
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
        "the inherited default must be visible to the hook"
    );
    assert!(agent.system_prompt.ends_with("Cite ticket IDs."));
}

/// A hook that throws must not take the session down, nor stop later hooks.
#[tokio::test]
async fn a_failing_start_hook_does_not_break_the_session() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.on_session_start(function(session) error("boom") end)
           cru.on_session_start(function(session)
             session.system_prompt = "second hook ran"
           end)"#,
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
        agent.system_prompt, "second hook ran",
        "one hook's failure must not skip another's setup"
    );
}

/// The shipped auto-approve must be OVERRIDABLE. Before priority existed the
/// gate was first-match-wins in registration order, the defaults always loaded
/// first, and a user hook could never win — while the defaults file's own
/// comment offered exactly this override as an example.
#[tokio::test]
async fn a_user_hook_overrides_the_shipped_auto_approve() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.permissions.on_request(function(request)
             if request.tool_name == "bash" then return { deny = true } end
             return nil
           end)"#,
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
    let agent_manager = Arc::new(create_test_agent_manager(session_manager));

    let result = run_permission_hooks(&agent_manager, &session.id, tool_request("auto")).await;

    assert_eq!(
        result,
        PermissionHookResult::Deny,
        "a user hook registers later but at a lower priority, so it is asked first"
    );
}

/// The shipped hook must stay behind user hooks. Registering it at the default
/// priority would silently restore the un-overridable behaviour, and the test
/// above would then be the only thing standing between us and that regression.
#[tokio::test]
async fn the_shipped_permission_hook_registers_behind_user_hooks() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;
    let state = agent_manager.get_or_create_session_state(&session_id);
    let guard = state.lock().await;
    let hooks = guard.permission_hooks.lock().unwrap();

    assert!(!hooks.is_empty(), "the shipped defaults register hooks");
    for hook in hooks.iter() {
        assert_eq!(
            hook.priority,
            crucible_lua::SHIPPED_DEFAULT_PRIORITY,
            "EVERY shipped hook must register behind the priority users get by \
             default ({}); one at 100 would be un-overridable again",
            hook.name
        );
    }
}

// ── Modes declared in Lua ────────────────────────────────────────────────

#[tokio::test]
async fn the_shipped_modes_are_declared_in_lua() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let modes = agent_manager.session_modes(&session_id);
    let ids: Vec<String> = modes
        .available_modes
        .iter()
        .map(|m| m.id.0.to_string())
        .collect();

    assert_eq!(
        ids,
        vec!["normal", "plan", "auto"],
        "the built-ins now come from runtime/defaults/init.lua, in declaration order"
    );
}

/// A mode a user invents must be selectable. Before the registry, `set_mode`
/// validated against a hardcoded list, so a mode you could define was a mode
/// you could never enter.
#[tokio::test]
async fn a_user_defined_mode_can_be_selected() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"cru.modes.review = {
             description = "Read-only review",
             tools = { "read_*", "*_search" },
             permissions = "ask",
           }"#,
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
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager
        .set_mode(&session.id, "review", None)
        .await
        .expect("a Lua-declared mode must be selectable");

    assert_eq!(
        session_manager
            .get_session(&session.id)
            .unwrap()
            .agent
            .unwrap()
            .mode
            .as_deref(),
        Some("review")
    );
}

#[tokio::test]
async fn an_undeclared_mode_is_still_rejected() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;

    let err = agent_manager
        .set_mode(&session_id, "nonsense", None)
        .await
        .expect_err("an undeclared mode must be rejected");

    assert!(err.to_string().contains("unknown mode"), "got: {err}");
}

/// A mode can be deleted outright, which is the point of shadowing the
/// defaults file rather than merely layering on top of it.
#[tokio::test]
async fn a_shipped_mode_can_be_removed() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(lua_dir.join("init.lua"), "cru.modes.auto = nil").unwrap();

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

    let ids: Vec<String> = agent_manager
        .session_modes(&session.id)
        .available_modes
        .iter()
        .map(|m| m.id.0.to_string())
        .collect();
    assert_eq!(ids, vec!["normal", "plan"]);
}

/// The stance replaces the hand-written auto/plan permission hooks.
#[test_case::test_case("auto", PermissionHookResult::Prompt; "auto defers to the stance, not a hook")]
#[test_case::test_case("normal", PermissionHookResult::Prompt; "normal prompts")]
#[tokio::test]
async fn shipped_modes_register_no_permission_hooks(mode: &str, expected: PermissionHookResult) {
    // With modes carrying the stance, the defaults file registers NO permission
    // hooks at all — the auto/plan behaviour is data now. Hooks stay available
    // for conditional policy, which is why "Prompt" (no hook had an opinion) is
    // the right answer here; the stance is applied later, in the gate.
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;
    let state = agent_manager.get_or_create_session_state(&session_id);
    let guard = state.lock().await;
    let hooks = guard.permission_hooks.lock().unwrap();
    let functions = guard.permission_functions.lock().unwrap();

    let result =
        crucible_lua::execute_permission_hooks(&guard.lua, &hooks, &functions, &tool_request(mode))
            .unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn the_auto_mode_stance_is_allow_and_plan_is_deny() {
    let (_tmp, agent_manager, session_id) = session_with_defaults().await;
    let _vm = agent_manager.get_or_create_session_state(&session_id);

    assert_eq!(
        agent_manager.mode_stance("auto"),
        Some(crucible_lua::ModeStance::Allow)
    );
    assert_eq!(
        agent_manager.mode_stance("plan"),
        Some(crucible_lua::ModeStance::Ask),
        "plan's real rule is conditional on is_safe, so it lives in a hook; a \
         blunt deny stance would refuse reads an agent card pushed through"
    );
    assert_eq!(
        agent_manager.mode_stance("normal"),
        Some(crucible_lua::ModeStance::Ask)
    );
}

/// The gap a tool-name glob cannot close: a mode that may run bash, but only
/// to search. Evaluated with the shared permission engine, so the rules mean
/// exactly what they mean in `[permissions]`.
#[test_case::test_case("rg pattern src/", PermissionDecision::Allow; "an allowed search command")]
#[test_case::test_case("grep -r foo .", PermissionDecision::Allow; "the other allowed one")]
#[test_case::test_case(
    "rm -rf build",
    PermissionDecision::Deny { reason: String::new() };
    "anything else falls to the mode default"
)]
#[test_case::test_case(
    "rg foo && rm -rf /",
    PermissionDecision::Deny { reason: String::new() };
    "a chained command cannot ride in on the allowed prefix"
)]
fn a_mode_can_permit_bash_for_specific_commands_only(command: &str, expected: PermissionDecision) {
    let permissions = crucible_lua::ModePermissions {
        default: crucible_lua::ModeStance::Deny,
        allow: vec!["bash:rg *".to_string(), "bash:grep *".to_string()],
        deny: Vec::new(),
        ask: Vec::new(),
    };

    let decision = AgentManager::evaluate_mode_rules(
        &permissions,
        "bash",
        &serde_json::json!({ "command": command }),
    );

    // Deny carries a reason string that varies by rule; compare the variant.
    assert_eq!(
        std::mem::discriminant(&decision),
        std::mem::discriminant(&expected),
        "command {command:?} produced {decision:?}"
    );
}
