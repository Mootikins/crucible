use super::*;

#[test]
fn init_lua_builtin_loads_without_error() {
    let lua = Lua::new();

    if let Err(e) = register_crucible_on_api(
        &lua,
        LuaScriptHandlerRegistry::new().runtime_handlers(),
        LuaScriptHandlerRegistry::new().handler_functions(),
    ) {
        panic!("register_crucible_on_api failed: {e}");
    }

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
