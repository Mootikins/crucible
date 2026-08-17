use super::*;
use crate::test_support::temp_session_manager;

/// Fast syntax + API gate: the shipped defaults must load against exactly the
/// surface the daemon session VM registers, and no more.
///
/// The old version of this test registered ONLY `crucible.on` and passed for
/// the entire life of a default that was guarded behind
/// `type(crucible.on_session_start) == "function"` and therefore never ran.
/// Those guards are gone — a shipped default that reaches for a missing API is
/// now a load error, which is what makes this test meaningful. Keep the
/// registrations here in sync with `get_or_create_session_state`; behavioural
/// coverage lives in `init_lua_defaults.rs`.
#[test]
fn init_lua_builtin_loads_against_the_session_vm_surface() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .expect("register_crucible_on_api failed");
    register_permission_hook_api(
        &lua,
        Arc::new(StdMutex::new(Vec::new())),
        Arc::new(StdMutex::new(HashMap::new())),
    )
    .expect("register_permission_hook_api failed");
    crucible_lua::register_session_defaults(&lua, crucible_lua::SessionDefaults::new())
        .expect("register_session_defaults failed");
    crucible_lua::register_modes(&lua, crucible_lua::ModeRegistry::new())
        .expect("register_modes failed");

    lua.load(crucible_lua::BUILTIN_INIT_LUA)
        .exec()
        .expect("built-in init.lua should load without error");
}

#[tokio::test]
async fn init_lua_user_override_loads_in_session() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(lua_dir.join("init.lua"), "test_override_loaded = true").unwrap();

    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            // Explicit: a user `init.lua` is discovered under the session's
            // WORKSPACE, and this fixture writes it into `tmp`. It used to
            // arrive there by the `workspace == kilns[0]` sentinel, which said
            // nothing about where the user meant to be working.
            Some(tmp.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let state = agent_manager.get_or_create_session_state(&session.id);
    let guard = state.lock().await;

    let val: bool = guard
        .lua
        .globals()
        .get::<bool>("test_override_loaded")
        .expect("user init.lua global should be readable");
    assert!(
        val,
        "user init.lua should have set test_override_loaded = true"
    );
}

/// `session.isolation` must read the same in a user's `cru.on_session_start`
/// as in a plugin's `crucible.on_session_start`.
///
/// The forwarding into the per-session VM was written but never exercised, so
/// the field could have silently read `nil` here while working for plugins —
/// one documented field with two surfaces that disagree, which is worse than
/// it existing on only one of them.
#[tokio::test]
async fn a_session_start_hook_sees_the_sessions_isolation_param() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"
        cru.on_session_start(function(session)
          seen_isolation = session.isolation
          seen_workspace = session.workspace
        end)
        "#,
    )
    .unwrap();

    let session_manager = temp_session_manager();
    let mut session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            // Explicit: a user `init.lua` is discovered under the session's
            // WORKSPACE, and this fixture writes it into `tmp`. It used to
            // arrive there by the `workspace == kilns[0]` sentinel, which said
            // nothing about where the user meant to be working.
            Some(tmp.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    // The same second write `session.create` does for the isolation opt-in.
    session.isolation = Some(serde_json::json!("rust"));
    session_manager.update_session(&session).await.unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let state = agent_manager.get_or_create_session_state(&session.id);
    let guard = state.lock().await;

    assert_eq!(
        guard.lua.globals().get::<String>("seen_isolation").ok(),
        Some("rust".to_string()),
        "the isolation param must reach a session's own start hook"
    );
    assert_eq!(
        guard.lua.globals().get::<String>("seen_workspace").ok(),
        Some(tmp.path().to_string_lossy().into_owned()),
        "and so must the workspace it is paired with"
    );
}
