use super::*;

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
