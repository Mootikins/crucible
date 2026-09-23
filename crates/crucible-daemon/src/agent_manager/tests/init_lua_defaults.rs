//! The behaviour `defaults/init.lua` is responsible for, asserted against the
//! REAL daemon VM, wired the way `bind_with_plugin_config` wires it, rather than a
//! hand-built `Lua`.
//!
//! Why that distinction matters: the previous default system prompt was
//! written as a `cru.on_session_start` hook, but that API is registered
//! only by `LuaExecutor` — never on the VM that ran the file. The guard
//! `if type(cru.on_session_start) == "function"` was therefore always
//! false and the prompt silently never applied, while `init_lua.rs`'s
//! "loads without error" test stayed green throughout. These tests assert the
//! effect, so a default that registers against a missing API fails loudly.

use super::*;
use crate::test_support::{kiln_name, temp_session_manager};
use crucible_core::config::components::permissions::PermissionDecision;
use crucible_lua::{execute_permission_hooks, PermissionHookResult, PermissionRequest};

/// A daemon VM that ran `BUILTIN_INIT_LUA` and then `extra`, plus a session
/// wired to it exactly as `Server::bind_with_plugin_config` wires production.
///
/// `extra` stands in for `~/.config/crucible/init.lua`, which runs second, so
/// an assignment in it wins and `cru.modes.x = nil` removes.
///
/// One VM runs every file. Sessions run none, so planting a file for a
/// session to find would prove nothing.
async fn session_with_lua(
    extra: &str,
) -> (
    crate::daemon_plugins::DaemonPluginLoader,
    Arc<AgentManager>,
    Arc<SessionManager>,
    String,
) {
    // The seed `daemon_plugins::boot` lays down before any Lua runs.
    // `chat.system_prompt` lives on that Default layer now, so a fixture that
    // reads or extends it needs the same seed production has.
    crucible_lua::seed_app_config(
        serde_json::to_value(crucible_core::config::CliAppConfig::default())
            .expect("serialize default config"),
    );
    let mut loader =
        crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
            .expect("daemon VM");
    let src = format!("{}\n{}", crucible_lua::BUILTIN_INIT_LUA, extra);
    loader
        .executor()
        .lua()
        .load(&src)
        .set_name("test defaults")
        .exec()
        .expect("the defaults file and the fixture must load");

    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");
    let agent_manager = Arc::new(
        Arc::into_inner(create_test_agent_manager(session_manager.clone()))
            .expect("the fixture hands out the only reference")
            .with_modes(Some(loader.mode_registry())),
    );
    agent_manager.set_daemon_permissions(loader.permission_registry());
    agent_manager.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
    // The same call `SessionLifecycle` makes at session create — the one fire
    // site. Driving it here rather than reimplementing it keeps the harness
    // from drifting away from what production does.
    crate::session_lifecycle::fire_start_hooks(
        &mut loader,
        Some(&agent_manager),
        &session_manager,
        &session.id,
    )
    .await
    .expect("start hooks must not refuse the session");

    let id = session.id.to_string();
    (loader, agent_manager, session_manager, id)
}

/// Run the daemon VM's permission hooks the way the tool gate does.
fn run_permission_hooks(
    loader: &crate::daemon_plugins::DaemonPluginLoader,
    request: &PermissionRequest,
) -> PermissionHookResult {
    let (hooks, lua) = loader.permission_registry();
    assert!(
        !hooks
            .runtime_handlers_for(
                "permission:request",
                Some(&request.call.tool),
                crucible_lua::Firing::Sessionless
            )
            .is_empty(),
        "defaults/init.lua must register a permission hook on the daemon VM; \
         an empty list means cru.permissions.on_request was missing there"
    );
    execute_permission_hooks(&lua, &hooks, request, crucible_lua::Firing::Sessionless).unwrap()
}

fn tool_request(mode: &str) -> PermissionRequest {
    PermissionRequest {
        call: crucible_core::types::CanonicalToolCall::crucible_tool(
            "bash",
            &serde_json::json!({ "command": "rm -rf build" }),
        ),
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
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_permission_hooks(&vm, &tool_request("auto"));

    assert_eq!(result, PermissionHookResult::Prompt);
}

#[tokio::test]
async fn normal_mode_still_reaches_the_prompt() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_permission_hooks(&vm, &tool_request("ask"));

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
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_permission_hooks(&vm, &tool_request("plan"));

    assert_eq!(result, PermissionHookResult::Deny);
}

/// An agent card's `ask` policy can push a read-only tool through the gate.
/// Denying it in plan mode would be wrong — plan mode forbids mutation, not
/// reading — which is why the request carries the daemon's own `is_safe`
/// classification instead of the hook assuming.
#[tokio::test]
async fn plan_mode_does_not_deny_a_read_only_tool() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let mut request = tool_request("plan");
    request.call = crucible_core::types::CanonicalToolCall::crucible_tool(
        "read_file",
        &serde_json::Value::Null,
    );
    request.is_safe = true;

    let result = run_permission_hooks(&vm, &request);

    assert_eq!(
        result,
        PermissionHookResult::Prompt,
        "plan mode forbids mutation, not reading"
    );
}

/// A permission hook reads the kind and the command line of the call, so
/// one hook decides the shell of each agent.
#[tokio::test]
async fn a_permission_hook_sees_the_kind_and_the_command() {
    let (vm, _am, _sm, id) = session_with_lua(
        r#"
        cru.permissions.on_request(function(request)
          if request.kind == "command" and request.command == "cargo test" then
            return { allow = true }
          end
          return nil
        end)
        "#,
    )
    .await;

    let args = serde_json::json!({ "command": "cargo test" });
    let result = AgentManager::run_permission_hooks(
        Some(&vm.permission_registry()),
        &crucible_core::types::CanonicalToolCall::crucible_tool("bash", &args),
        &args,
        &id,
        "ask",
        &std::collections::HashSet::new(),
    );

    assert_eq!(result, PermissionHookResult::Allow);
}

/// A hook that cannot see the mode is the failure this plumbing exists to
/// prevent: it would fall through to Prompt for every request, and auto mode
/// would look like it "just doesn't work".
#[tokio::test]
async fn request_without_a_mode_falls_through_to_the_prompt() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let mut request = tool_request("auto");
    request.mode = None;

    let result = run_permission_hooks(&vm, &request);

    assert_eq!(result, PermissionHookResult::Prompt);
}

/// `configure_agent` is where a default becomes real, so the assertions run
/// through it rather than through the Lua store — a value that reaches
/// the start scope but never reaches `AgentConfig` is exactly the failure
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
    agent
}

#[tokio::test]
async fn an_agent_with_no_prompt_of_its_own_gets_the_default() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;
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
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;
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
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;
    let session_manager = agent_manager.session_manager.clone();

    let mut agent = bare_agent();
    agent.system_prompt = "You are a haiku bot.".to_string();

    let configured = configured_agent(&agent_manager, &session_manager, &session_id, agent).await;

    assert_eq!(configured.system_prompt, "You are a haiku bot.");
}

/// The append idiom, end to end: a user file extends the shipped prompt
/// instead of replacing it, using plain string concatenation.
#[tokio::test]
async fn a_user_init_lua_can_append_to_a_shipped_default() {
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.config.set {
             chat = {
               system_prompt = cru.config.get("chat").system_prompt
                 .. "\n\nAnswer in British English.",
             },
           }"#,
    )
    .await;

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

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
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.on_session_start(function(session)
             session.system_prompt = "per-session prompt"
           end)"#,
    )
    .await;

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    assert_eq!(agent.system_prompt, "per-session prompt");
}

/// A hook can choose the model the agent starts with. The hook runs after
/// the caller named a model, so its choice replaces that one.
#[tokio::test]
async fn on_session_start_can_pick_the_model() {
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.on_session_start(function(session)
             session.model = "hook-picked-model"
           end)"#,
    )
    .await;

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    assert_eq!(agent.model, "hook-picked-model");
}

/// The hook reads the INHERITED value before overriding it — the Neovim
/// pattern where a `FileType` autocmd sees the global option and sets the
/// buffer-local one. Without seeding, `session.system_prompt` would be nil
/// inside the hook and appending would error.
#[tokio::test]
async fn on_session_start_sees_the_global_default_and_can_extend_it() {
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.on_session_start(function(session)
             session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
           end)"#,
    )
    .await;

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    assert!(
        agent.system_prompt.contains("Crucible"),
        "the inherited default must be visible to the hook"
    );
    assert!(agent.system_prompt.ends_with("Cite ticket IDs."));
}

/// A hook that throws must not take the session down, nor stop later hooks.
#[tokio::test]
async fn a_failing_start_hook_does_not_break_the_session() {
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.on_session_start(function(session) error("boom") end)
           cru.on_session_start(function(session)
             session.system_prompt = "second hook ran"
           end)"#,
    )
    .await;

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    assert_eq!(
        agent.system_prompt, "second hook ran",
        "one hook's failure must not skip another's setup"
    );
}

/// A user hook must still be REACHED. The gate answers on the first hook that
/// decides, and the shipped defaults load before any user file, so every
/// shipped hook that declines is what leaves a user hook reachable at all.
#[tokio::test]
async fn a_user_hook_is_reached_in_a_mode_the_defaults_decline() {
    let (vm, _am, _sm, _id) = session_with_lua(
        r#"cru.permissions.on_request(function(request)
             if request.tool_name == "bash" then return { deny = true } end
             return nil
           end)"#,
    )
    .await;

    let result = run_permission_hooks(&vm, &tool_request("auto"));

    assert_eq!(
        result,
        PermissionHookResult::Deny,
        "the shipped hook answers nil for `auto`, so the user's hook decides"
    );
}

/// The shipped defaults must DECLINE for every mode but `plan`.
///
/// This replaces a gate that read `hook.priority` against a
/// `SHIPPED_DEFAULT_PRIORITY` constant. Both are gone: handlers run in
/// registration order and nothing reorders them, so "the shipped hook is
/// asked last" is no longer expressible and no longer true — it is asked
/// FIRST. What keeps a user hook reachable is therefore the hook BODY, and
/// this asserts that instead of an ordering number.
///
/// `plan` is the exception on purpose. Its deny is the one shipped decision
/// that must hold, and the daemon enforces plan mode independently anyway.
///
/// The mode list comes from the running registry, not from a literal, so a
/// mode added to `defaults/init.luau` is covered without editing this test.
#[tokio::test]
async fn the_shipped_permission_hooks_decline_for_every_mode_but_plan() {
    let (vm, _am, _sm, _session_id) = session_with_lua("").await;

    for mode in vm.mode_registry().all() {
        if mode.name == "plan" {
            continue;
        }
        assert_eq!(
            run_permission_hooks(&vm, &tool_request(&mode.name)),
            PermissionHookResult::Prompt,
            "no shipped hook may decide for mode `{}`: it is asked first, so a \
             decision here is final and no user hook could ever override it",
            mode.name
        );
    }
}

// ── Modes declared in Lua ────────────────────────────────────────────────

#[tokio::test]
async fn the_shipped_modes_are_declared_in_lua() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;

    let modes = agent_manager.session_modes(&session_id);
    let ids: Vec<String> = modes
        .available_modes
        .iter()
        .map(|m| m.id.0.to_string())
        .collect();

    assert_eq!(
        ids,
        vec!["ask", "plan", "auto", "propose"],
        "the built-ins now come from runtime/defaults/init.lua, in declaration order"
    );
}

/// The label a front end renders comes from the declaration, not from
/// capitalising the id here. Both front ends show `name`, so the derivation
/// is what a user actually reads for any mode that did not declare one.
#[tokio::test]
async fn a_declared_modes_label_reaches_the_descriptor() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua(
        r#"cru.modes.acceptEdits = { permissions = "ask" }
           cru.modes.deepReview = { label = "Deep review", permissions = "ask" }"#,
    )
    .await;

    let labels: Vec<(String, String)> = agent_manager
        .session_modes(&session_id)
        .available_modes
        .iter()
        .map(|m| (m.id.0.to_string(), m.name.clone()))
        .collect();

    let label_of = |id: &str| {
        labels
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| panic!("mode {id} missing from {labels:?}"))
    };

    assert_eq!(label_of("ask"), "Ask", "the shipped modes declare theirs");
    assert_eq!(label_of("plan"), "Plan");
    assert_eq!(
        label_of("acceptEdits"),
        "Accept edits",
        "an id with a word boundary must not reach a front end as `AcceptEdits`"
    );
    assert_eq!(
        label_of("deepReview"),
        "Deep review",
        "a declared label wins over the derived one"
    );
}

/// `ask` was `normal` until the id was made to name its stance. The id is
/// persisted on a session and the registry has no fallback, so a session
/// written before the rename would fail to resolve its mode rather than
/// degrade — the alias is what stops that.
#[tokio::test]
async fn the_former_normal_id_still_resolves_to_ask() {
    let (_vm, agent_manager, _sm, _session_id) = session_with_lua("").await;
    // Declaring the modes is what loading the session's VM does.

    assert_eq!(
        agent_manager.mode_stance("normal"),
        agent_manager.mode_stance("ask"),
        "a session that still names `normal` must get the mode it used to name"
    );
    assert!(
        agent_manager.mode_stance("normal").is_some(),
        "the alias must resolve to a real mode, not to None"
    );
}

/// The alias resolves, but it is not a mode. Listing it would show the user
/// the same mode twice and put a dead id in the mode cycle.
#[tokio::test]
async fn the_former_normal_id_is_not_offered_as_a_mode() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;

    let ids: Vec<String> = agent_manager
        .session_modes(&session_id)
        .available_modes
        .iter()
        .map(|m| m.id.0.to_string())
        .collect();

    assert!(
        !ids.contains(&"normal".to_string()),
        "the deprecated id must not be advertised; got {ids:?}"
    );
}

/// A mode a user invents must be selectable. Before the registry, `set_mode`
/// validated against a hardcoded list, so a mode you could define was a mode
/// you could never enter.
#[tokio::test]
async fn a_user_defined_mode_can_be_selected() {
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.modes.review = {
             description = "Read-only review",
             tools = { "read_*", "*_search" },
             permissions = "ask",
           }"#,
    )
    .await;
    agent_manager
        .configure_agent(&session_id, test_agent())
        .await
        .unwrap();

    agent_manager
        .set_mode(&session_id, "review", None)
        .await
        .expect("a Lua-declared mode must be selectable");

    assert_eq!(
        session_manager
            .get_session(&session_id)
            .unwrap()
            .agent
            .unwrap()
            .mode
            .as_deref(),
        Some("review")
    );
}

/// Accepting the old id is half the migration. Storing it would write the
/// dead spelling back onto the session and keep it alive forever, so the
/// canonical id is what lands.
#[tokio::test]
async fn selecting_the_former_normal_id_stores_the_canonical_one() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![kiln_name("kiln")],
            Some(tmp.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager
        .set_mode(&session.id, "normal", None)
        .await
        .expect("the id a pre-rename session persists must still be selectable");

    assert_eq!(
        session_manager
            .get_session(&session.id)
            .unwrap()
            .agent
            .unwrap()
            .mode
            .as_deref(),
        Some("ask"),
        "the session must be left naming the mode's current id, not the alias"
    );
}

#[tokio::test]
async fn an_undeclared_mode_is_still_rejected() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;

    let err = agent_manager
        .set_mode(&session_id, "nonsense", None)
        .await
        .expect_err("an undeclared mode must be rejected");

    assert!(err.to_string().contains("unknown mode"), "got: {err}");
}

/// A mode can be deleted outright, which is the point of a defaults file the
/// user can shadow rather than merely layer on top of.
#[tokio::test]
async fn a_shipped_mode_can_be_removed() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("cru.modes.auto = nil").await;

    let ids: Vec<String> = agent_manager
        .session_modes(&session_id)
        .available_modes
        .iter()
        .map(|m| m.id.0.to_string())
        .collect();
    assert_eq!(ids, vec!["ask", "plan", "propose"]);
}

/// The stance replaces the hand-written auto/plan permission hooks.
#[test_case::test_case("auto", PermissionHookResult::Prompt; "auto defers to the stance, not a hook")]
#[test_case::test_case("ask", PermissionHookResult::Prompt; "ask prompts")]
#[tokio::test]
async fn shipped_modes_register_no_permission_hooks(mode: &str, expected: PermissionHookResult) {
    // With modes carrying the stance, the defaults file registers NO permission
    // hooks at all — the auto/plan behaviour is data now. Hooks stay available
    // for conditional policy, which is why "Prompt" (no hook had an opinion) is
    // the right answer here; the stance is applied later, in the gate.
    let (vm, _am, _sm, _session_id) = session_with_lua("").await;
    let (hooks, lua) = vm.permission_registry();

    let result = crucible_lua::execute_permission_hooks(
        &lua,
        &hooks,
        &tool_request(mode),
        crucible_lua::Firing::Sessionless,
    )
    .unwrap();
    assert_eq!(result, expected);
}

#[tokio::test]
async fn the_auto_mode_stance_is_allow_and_plan_is_deny() {
    let (_vm, agent_manager, _sm, _session_id) = session_with_lua("").await;

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
        agent_manager.mode_stance("ask"),
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

    let args = serde_json::json!({ "command": command });
    let decision = AgentManager::evaluate_mode_rules(
        &permissions,
        &crucible_core::types::CanonicalToolCall::crucible_tool("bash", &args),
        &args,
    );

    // Deny carries a reason string that varies by rule; compare the variant.
    assert_eq!(
        std::mem::discriminant(&decision),
        std::mem::discriminant(&expected),
        "command {command:?} produced {decision:?}"
    );
}

/// `session_modes().current_mode_id` must be the SESSION's mode.
///
/// It was `declared.first()` — registration order — which nothing caught
/// because both callers read only `available_modes`. Putting that struct on
/// the wire would have shown every UI whichever mode the defaults file
/// declares first, which is the same "reports a state it does not have"
/// defect this whole arc is about.
#[tokio::test]
async fn session_modes_reports_the_sessions_own_current_mode() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;
    agent_manager
        .configure_agent(&session_id, test_agent())
        .await
        .unwrap();

    // "ask" is declared FIRST in runtime/defaults/init.lua, so selecting
    // anything else is what distinguishes the session's mode from the
    // registration order.
    agent_manager
        .set_mode(&session_id, "plan", None)
        .await
        .unwrap();

    let modes = agent_manager.session_modes(&session_id);
    assert_eq!(
        modes.current_mode_id.0.as_ref(),
        "plan",
        "must be the session's mode, not the first declared one"
    );
}

/// A persisted mode that is no longer declared must not be reported as
/// current — that would put a mode nobody can enforce on the wire.
#[tokio::test]
async fn session_modes_ignores_a_persisted_mode_that_no_longer_exists() {
    let (_vm, agent_manager, _sm, session_id) = session_with_lua("").await;
    let session_manager = agent_manager.session_manager.clone();
    agent_manager
        .configure_agent(&session_id, test_agent())
        .await
        .unwrap();

    agent_manager
        .set_mode(&session_id, "plan", None)
        .await
        .unwrap();

    // Simulate the declaration going away underneath a persisted session.
    agent_manager.modes.remove("plan");

    let modes = agent_manager.session_modes(&session_id);
    assert_ne!(
        modes.current_mode_id.0.as_ref(),
        "plan",
        "an undeclared mode must not be reported as current"
    );
    assert!(
        modes
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == modes.current_mode_id.0.as_ref()),
        "current_mode_id must always be one of available_modes"
    );
    // The persisted value is untouched; this is a reporting fix, not a writer.
    assert_eq!(
        session_manager
            .get_session(&session_id)
            .unwrap()
            .agent
            .unwrap()
            .mode
            .as_deref(),
        Some("plan")
    );
}

/// A mode chosen in an `on_session_start` hook reaches the agent.
///
/// `session.mode = "plan"` had no backing on `SessionStartScopeRpc`, so it hit
/// the trait default. That default used to be `Ok(())` — silently inert — and
/// then became an error, which aborts the hook body and takes every line after
/// it down too. Neither is a session that starts in plan mode. Asserted
/// through `configure_agent` for the reason the helper above gives: a value
/// that reaches the start scope and not `SessionAgent` is the failure mode.
#[tokio::test]
async fn a_mode_set_in_a_session_start_hook_reaches_the_agent() {
    let (_vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"cru.on_session_start(function(session)
             session.mode = "plan"
           end)"#,
    )
    .await;

    let agent = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;

    assert_eq!(
        agent.mode.as_deref(),
        Some("plan"),
        "the hook's mode must be what the agent starts as"
    );
}

/// …and an agent that named its own mode keeps it. Defaults fill gaps; they
/// do not overrule a choice already made.
#[tokio::test]
async fn an_agents_own_mode_outranks_the_default() {
    let (_vm, agent_manager, _sm, session_id) =
        session_with_lua(r#"cru.on_session_start(function(session) session.mode = "plan" end)"#)
            .await;
    let session_manager = agent_manager.session_manager.clone();

    let mut agent = bare_agent();
    agent.mode = Some("ask".to_string());
    let agent = configured_agent(&agent_manager, &session_manager, &session_id, agent).await;

    assert_eq!(agent.mode.as_deref(), Some("ask"));
}

/// A start hook runs ONCE per session.
///
/// It briefly ran twice. Once the two VMs became one, `SessionLifecycle` fired
/// the hooks at session create and `build_session_state` fired the same table
/// again at the session's first turn — so a plugin that builds a container in
/// `on_session_start` built two, and the two sites bound the session
/// differently, so the same hook body could succeed at one and raise at the
/// other. Counting in Lua is the only assertion that catches it: every
/// observable EFFECT of firing twice is idempotent.
#[tokio::test]
async fn a_start_hook_runs_once_per_session() {
    let (vm, agent_manager, session_manager, session_id) = session_with_lua(
        r#"fired = 0
           cru.on_session_start(function(session)
             fired = fired + 1
           end)"#,
    )
    .await;

    // Everything a turn does that used to re-fire.
    let _ = configured_agent(&agent_manager, &session_manager, &session_id, bare_agent()).await;
    let _ = agent_manager.session_modes(&session_id);

    let fired: i64 = vm.plugin_lua().globals().get("fired").unwrap();
    assert_eq!(
        fired, 1,
        "the hook must fire once per session, not once per site"
    );
}

/// Run the daemon VM's `tool_result` handlers exactly as the seam does over a
/// finished outcome: every handler registered for the tool, in registration
/// order, each seeing the previous handlers' patches. Error results never
/// reach the seam (`tool_call.rs` runs it only when `error.is_none()`), so
/// this helper models the success arm only.
async fn run_tool_result_handlers(
    loader: &crate::daemon_plugins::DaemonPluginLoader,
    tool: &str,
    args: serde_json::Value,
    result: &str,
) -> String {
    let handlers = loader.plugin_handlers();
    let lua = loader.plugin_lua();
    let event = crucible_core::events::SessionEvent::Custom {
        name: "tool_result".to_string(),
        payload: serde_json::json!({
            "tool": tool,
            "args": args,
            "result": result,
            "error": Option::<String>::None,
        }),
    };
    let mut patched = result.to_string();
    for handler in handlers.runtime_handlers_for(
        crucible_lua::StageId::ToolResult.as_str(),
        Some(tool),
        crucible_lua::Firing::InSession("test-session"),
    ) {
        let crucible_lua::ScriptHandlerResult::Transform(val) = handlers
            .execute_runtime_handler(&lua, handler.id, &event, Some("test-session"))
            .await
            .expect("every registered tool_result handler must run")
        else {
            continue;
        };
        if let Some(new_result) = val.get("result").and_then(|v| v.as_str()) {
            patched = new_result.to_string();
        }
    }
    patched
}

/// The shipped formatter is scoped to the bash tool. `pattern = "bash"` is
/// the whole scoping mechanism — no Lua-side guard could rescue a pattern
/// that never matches.
#[tokio::test]
async fn the_bash_result_formatter_is_registered_for_bash_only() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let handlers = vm.plugin_handlers();
    assert!(
        !handlers
            .runtime_handlers_for(
                crucible_lua::StageId::ToolResult.as_str(),
                Some("bash"),
                crucible_lua::Firing::InSession("test-session"),
            )
            .is_empty(),
        "defaults/init.lua must register a tool_result handler for bash"
    );
    assert!(
        handlers
            .runtime_handlers_for(
                crucible_lua::StageId::ToolResult.as_str(),
                Some("read_file"),
                crucible_lua::Firing::InSession("test-session"),
            )
            .is_empty(),
        "the formatter must stay scoped to bash; read_file results are not terminal output"
    );
}

/// The formatter renders the result the way a terminal would: the command
/// echoed, the output verbatim beneath it. The echo is also what stops the
/// web card from pretty-printing JSON-bearing output as a {...} object — the
/// prefixed string no longer parses as bare JSON.
#[tokio::test]
async fn the_shipped_bash_formatter_echoes_the_command_over_the_output() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_tool_result_handlers(
        &vm,
        "bash",
        serde_json::json!({ "command": "ls src" }),
        "total 42\nmain.rs\n",
    )
    .await;

    assert_eq!(result, "$ ls src\ntotal 42\nmain.rs\n");
}

/// Output that happens to be JSON stays text under the command line, exactly
/// as the terminal printed it.
#[tokio::test]
async fn the_shipped_bash_formatter_keeps_json_output_as_text() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_tool_result_handlers(
        &vm,
        "bash",
        serde_json::json!({ "command": "cru models" }),
        r#"{"models":["glm-5.3-flash"]}"#,
    )
    .await;

    assert_eq!(result, "$ cru models\n{\"models\":[\"glm-5.3-flash\"]}");
}

/// A non-zero exit is NOT a tool error — the daemon returns it as success
/// text ("Exit code: N / Stdout: / Stderr:"), and the formatter keeps that
/// block verbatim beneath the command line.
#[tokio::test]
async fn the_shipped_bash_formatter_keeps_the_failure_block_verbatim() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_tool_result_handlers(
        &vm,
        "bash",
        serde_json::json!({ "command": "cargo build" }),
        "Exit code: 101\nStdout:\n\nStderr:\nerror: build failed\n",
    )
    .await;

    assert_eq!(
        result,
        "$ cargo build\nExit code: 101\nStdout:\n\nStderr:\nerror: build failed\n"
    );
}

/// A command that printed nothing would leave the command line alone to read
/// as a truncated result, so silence is stated.
#[tokio::test]
async fn the_shipped_bash_formatter_marks_silent_success() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result =
        run_tool_result_handlers(&vm, "bash", serde_json::json!({ "command": "true" }), "").await;

    assert_eq!(result, "$ true\n(no output)");
}

/// No `command` in the args — a foreign tool wearing the bash name — gets no
/// formatting: the handler declines and the result passes through untouched.
#[tokio::test]
async fn the_shipped_bash_formatter_declines_without_a_command_argument() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;

    let result = run_tool_result_handlers(&vm, "bash", serde_json::json!({}), "raw").await;

    assert_eq!(result, "raw");
}

/// The Lua layers decide an ACP permission request as they decide a call of
/// Crucible's own tools: a `permission:request` hook, the stance of a mode,
/// and the plan mode rule of the defaults.
#[tokio::test]
async fn the_lua_layers_decide_an_acp_request() {
    use agent_client_protocol::schema::v1::{PermissionOption, PermissionOptionKind};
    let (_vm, am, _sm, id) = session_with_lua(
        r#"
        cru.permissions.on_request(function(request)
          if request.kind == "command" then return { deny = true } end
        end)
        cru.modes.locked = { tools = "*", permissions = "deny" }
        "#,
    )
    .await;
    let (event_tx, mut event_rx) = broadcast::channel(16);
    let handle = am.build_acp_permission_handler(&id, &event_tx, std::path::Path::new("/w"), None);
    let acp = |frame: serde_json::Value| {
        crucible_core::types::classify_acp(serde_json::from_value(frame).unwrap(), &[])
    };
    let command = acp(serde_json::json!({
        "kind": "execute", "title": "Run", "rawInput": {"command": "cargo test"},
    }));
    let edit = acp(serde_json::json!({
        "kind": "edit", "title": "Edit", "locations": [{"path": "/w/src/lib.rs"}],
    }));
    let options = || {
        vec![
            PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce),
            PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
        ]
    };

    for (mode, call, layer) in [
        ("ask", command, "the Lua hook"),
        ("locked", edit.clone(), "the mode stance"),
        ("plan", edit, "the plan rule"),
    ] {
        am.slot(&id)
            .set_turn_gate(crate::agent_manager::slot::TurnGate {
                is_interactive: true,
                permission_override: None,
                mode: mode.to_string(),
                ..Default::default()
            });
        let outcome =
            tokio::time::timeout(std::time::Duration::from_secs(5), handle(call, options()))
                .await
                .unwrap_or_else(|_| panic!("{layer} must answer without a prompt"));
        assert!(
            matches!(&outcome,
                agent_client_protocol::schema::v1::RequestPermissionOutcome::Selected(s)
                    if s.option_id.to_string() == "reject"),
            "{layer} must refuse: {outcome:?}"
        );
        assert!(event_rx.try_recv().is_err(), "{layer} asks nobody");
    }
}

/// Render `call` with the handlers of `vm`, as the daemon renders a tool
/// call event.
async fn render(
    vm: &crate::daemon_plugins::DaemonPluginLoader,
    mut call: crucible_core::types::CanonicalToolCall,
    args: serde_json::Value,
) -> crucible_core::types::ToolRender {
    let handlers = (vm.plugin_handlers(), vm.plugin_lua());
    crate::agent_manager::messaging::tool_hooks::render_call(
        Some(&handlers),
        "test-session",
        &mut call,
        &args,
        crucible_core::turn::TurnOrigin::User,
    )
    .await;
    call.render.expect("render_call always sets a render")
}

/// An ACP call with `raw` as its wire fields, classified with no key table.
fn acp_call(raw: serde_json::Value) -> crucible_core::types::CanonicalToolCall {
    crucible_core::types::classify_acp(serde_json::from_value(raw).unwrap(), &[])
}

/// Each built-in kind but `tool` renders through the Lua defaults, with the
/// field of its kind as the line.
#[tokio::test]
async fn each_builtin_kind_renders_through_the_lua_defaults() {
    use crucible_core::types::CanonicalToolCall;
    use serde_json::json;
    let (vm, _am, _sm, _id) = session_with_lua("").await;
    let crucible =
        |name: &str, args: serde_json::Value| (CanonicalToolCall::crucible_tool(name, &args), args);
    let cases = [
        (
            crucible("bash", json!({ "command": "ls -la" })),
            "command",
            "ls -la",
        ),
        (
            crucible("write_file", json!({ "path": "a.md" })),
            "file_edit",
            "a.md",
        ),
        (
            crucible("read_file", json!({ "path": "b.md" })),
            "file_read",
            "b.md",
        ),
        (
            crucible("semantic_search", json!({ "query": "links" })),
            "search",
            "links",
        ),
        (
            crucible("fetch", json!({ "url": "https://a.test" })),
            "fetch",
            "https://a.test",
        ),
        (
            (
                acp_call(json!({ "name": "mcp__github__create_pr", "title": "Create PR" })),
                json!(null),
            ),
            "mcp_tool",
            "Create PR",
        ),
    ];
    for ((call, args), kind, line) in cases {
        assert_eq!(call.kind, kind);
        let render = render(&vm, call, args).await;
        assert_eq!(render.line.as_deref(), Some(line), "{kind}");
        assert!(render.fields.is_empty(), "{kind}: the fallback did not run");
    }
}

/// The completeness gate: each kind that the matchers or the shipped agent
/// key tables give has a Lua render, except `tool`, which the fallback
/// renders.
#[tokio::test]
async fn each_kind_has_a_render_or_uses_the_fallback() {
    let (vm, _am, _sm, _id) = session_with_lua("").await;
    let table_kinds: Vec<String> = vm
        .plugin_lua()
        .load(
            r#"
            local kinds = {}
            for _, agent in cru.config.get("acp").agents do
              for _, entry in agent.tools or {} do
                table.insert(kinds, entry.kind)
              end
            end
            return kinds
            "#,
        )
        .eval()
        .expect("the key tables read");
    assert!(!table_kinds.is_empty(), "the shipped key tables name kinds");
    let kinds = crucible_core::types::CanonicalToolCall::KINDS
        .iter()
        .map(|k| k.to_string())
        .chain(table_kinds);
    let registry = vm.plugin_handlers();
    for kind in kinds {
        let renders = registry.for_hook(
            crucible_lua::StageId::ToolRender.into(),
            Some(&kind),
            crucible_lua::Firing::Sessionless,
        );
        assert_eq!(
            renders.is_empty(),
            kind == "tool",
            "kind `{kind}` needs a render in runtime/defaults/init.luau, or the fallback"
        );
    }
}

/// A plugin adds a kind and its render. The render gets the origin of the
/// turn. A kind with no render gets the fallback.
#[tokio::test]
async fn a_plugin_renders_its_own_kind() {
    let plugin = r#"
        cru.on("tool:render", { pattern = "delegate" }, function(ctx, call)
          return {
            line = call.raw.title,
            fields = { { label = "origin", value = call.origin.kind } },
          }
        end)
    "#;
    let mut call = acp_call(serde_json::json!({ "title": "Task: fix the bug" }));
    call.kind = "delegate".to_string();

    let (vm, _am, _sm, _id) = session_with_lua("").await;
    let fallback = render(&vm, call.clone(), serde_json::Value::Null).await;
    assert_eq!(
        fallback.fields[0].value, "delegate",
        "the fallback keeps the kind"
    );

    let (vm, _am, _sm, _id) = session_with_lua(plugin).await;
    let render = render(&vm, call, serde_json::Value::Null).await;
    assert_eq!(render.line.as_deref(), Some("Task: fix the bug"));
    assert_eq!(render.fields[0].value, "user");
}

/// The last render of a kind wins, so a user file replaces a shipped render.
#[tokio::test]
async fn a_user_render_replaces_the_shipped_one() {
    let (vm, _am, _sm, _id) = session_with_lua(
        r#"
        cru.on("tool:render", { pattern = "command" }, function(ctx, call)
          return { line = "$ " .. call.command }
        end)
        "#,
    )
    .await;
    let args = serde_json::json!({ "command": "ls" });
    let call = crucible_core::types::CanonicalToolCall::crucible_tool("bash", &args);
    assert_eq!(render(&vm, call, args).await.line.as_deref(), Some("$ ls"));
}

/// A render that fails gives the fallback, which shows every field.
#[tokio::test]
async fn a_failing_render_gives_the_fallback() {
    let (vm, _am, _sm, _id) = session_with_lua(
        r#"
        cru.on("tool:render", { pattern = "command" }, function(ctx, call)
          error("broken render")
        end)
        "#,
    )
    .await;
    let args = serde_json::json!({ "command": "ls" });
    let call = crucible_core::types::CanonicalToolCall::crucible_tool("bash", &args);
    let render = render(&vm, call, args).await;
    assert_eq!(render.fields[0].value, "command");
}
