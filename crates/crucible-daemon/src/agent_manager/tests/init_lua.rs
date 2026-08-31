use super::*;
use crate::test_support::temp_session_manager;

/// The session PROFILE — what `cru plugin stubs` renders the session
/// definitions file from — must describe the VM `AgentManager` really builds.
///
/// `init_lua_builtin_loads_against_the_session_vm_surface` below proves the
/// shipped defaults LOAD. It cannot prove the definitions file is right, and
/// those are different failures: the defaults loaded perfectly while the only
/// published stub file reported five type errors against them, because that
/// file described the daemon VM and the defaults run here.
///
/// Compared as SETS, both directions reported. A path the real VM has and the
/// profile lacks makes the checker reject working code. The reverse makes it
/// accept code that is nil at run time — the worse one, and the one a
/// "does it load" test never catches.
#[tokio::test]
async fn the_session_profile_matches_a_real_session_vm() {
    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();
    let agent_manager = create_test_agent_manager(session_manager);
    let state = agent_manager.get_or_create_session_state(&session.id);
    let guard = state.lock().await;
    let real = crate::vm_profiles::tests::surface(&guard.lua);
    drop(guard);

    let built = crate::vm_profiles::session_vm().expect("the session profile must build");
    let profile = crate::vm_profiles::tests::surface(&built);

    let missing: Vec<&String> = real.iter().filter(|p| !profile.contains(p)).collect();
    assert!(
        missing.is_empty(),
        "the session profile omits API a real session VM has, so the session \
         definitions file would report a type error for working code: {missing:?}"
    );

    // `cru.session.*` is registered only when boot bound a `DaemonSessionApi`
    // (`session_vm.rs`, `if let Some(api) = self.session_api()`). A booted
    // daemon always binds one, so it belongs in the definitions file; this
    // harness never boots, so the VM it builds lacks it. That ONE difference is
    // named here rather than waived wholesale — anything else the profile
    // invents still fails, which is the direction that would otherwise pass
    // code that is nil at run time.
    let invented: Vec<&String> = profile
        .iter()
        .filter(|p| !real.contains(p))
        .filter(|p| !p.starts_with("cru.session"))
        .collect();
    assert!(
        invented.is_empty(),
        "the session profile advertises API a real session VM does NOT have, so \
         the session definitions file would pass code that is nil at run time: \
         {invented:?}"
    );

    // And the waiver above must stay a waiver, not a hole: the profile really
    // does carry the session module, so the definitions file describes it.
    assert!(
        profile.iter().any(|p| p == "cru.session.complete"),
        "the session profile must carry cru.session.*, which a booted daemon has"
    );
}

/// Fast syntax + API gate: the shipped defaults must load against exactly the
/// surface the daemon session VM registers, and no more.
///
/// The old version of this test registered ONLY `cru.on` and passed for
/// the entire life of a default that was guarded behind
/// `type(cru.on_session_start) == "function"` and therefore never ran.
/// Those guards are gone — a shipped default that reaches for a missing API is
/// now a load error, which is what makes this test meaningful.
///
/// The VM here is deliberately a FLOOR, not a copy: it registers four of the
/// eight things `session_vm.rs` registers, so a default that loads here loads
/// in production too. The comment used to ask for the two to be kept in sync,
/// which was never true and would only weaken the test. The CEILING — that
/// the session profile describes the VM `AgentManager` really builds — is held
/// by `the_session_profile_matches_a_real_session_vm` above. Behavioural
/// coverage lives in `init_lua_defaults.rs`.
#[test]
fn init_lua_builtin_loads_against_the_session_vm_surface() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_cru_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .expect("register_cru_on_api failed");
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
/// as in a plugin's `cru.on_session_start`.
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

/// `session:set_variable` from a start hook lands in daemon storage, and the
/// hook reads it back after a resume.
///
/// The Lua surface existed for a long time with no store behind it: the
/// daemon bound an empty backing, so a plugin that stored a value read `nil`
/// back on the next session. The map now lives on the session slot and is
/// written into `meta.json`, so a second manager over the same storage sees
/// what the first one stored.
#[tokio::test]
async fn a_session_variable_set_by_a_start_hook_survives_a_resume() {
    let tmp = TempDir::new().unwrap();
    let lua_dir = tmp.path().join(".crucible/lua");
    std::fs::create_dir_all(&lua_dir).unwrap();
    std::fs::write(
        lua_dir.join("init.lua"),
        r#"
        cru.on_session_start(function(session)
          seen_before = session:get_variable("visits")
          session:set_variable("visits", (seen_before or 0) + 1)
          session:set_variable("shape", { nested = true })
          seen_after = session:get_variable("visits")
        end)
        "#,
    )
    .unwrap();

    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            Some(tmp.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    {
        let state = agent_manager.get_or_create_session_state(&session.id);
        let guard = state.lock().await;
        assert!(
            guard
                .lua
                .globals()
                .get::<mlua::Value>("seen_before")
                .unwrap()
                .is_nil(),
            "a fresh session has no variables"
        );
        assert_eq!(
            guard.lua.globals().get::<i64>("seen_after").ok(),
            Some(1),
            "the hook reads back what it stored in the same session"
        );
    }
    // The VM builder persists from a spawned task; wait for it to land.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while session_manager
        .get_session(&session.id)
        .is_some_and(|s| s.variables.is_empty())
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the start hook's variables never reached the session"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Reload from disk: a resume reads `meta.json`, not the in-memory entry.
    let resumed = session_manager
        .resume_session_from_storage(&session.id)
        .await
        .unwrap();
    assert_eq!(resumed.variables.get("visits"), Some(&serde_json::json!(1)));
    assert_eq!(
        resumed.variables.get("shape"),
        Some(&serde_json::json!({ "nested": true }))
    );

    // A second manager has fresh slots, so its VM seeds from the session.
    let second = create_test_agent_manager(session_manager.clone());
    let state = second.get_or_create_session_state(&session.id);
    let guard = state.lock().await;
    assert_eq!(
        guard.lua.globals().get::<i64>("seen_before").ok(),
        Some(1),
        "the resumed session's hook reads the persisted variable"
    );
    assert_eq!(guard.lua.globals().get::<i64>("seen_after").ok(), Some(2));
}
