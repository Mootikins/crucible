use super::*;
use crate::test_support::temp_session_manager;

/// A daemon VM that ran `BUILTIN_INIT_LUA` and then `extra`.
///
/// The daemon VM is the only VM that runs Lua files, so a hook fixture must
/// register here. `BUILTIN_INIT_LUA` is the prefix because the shipped file
/// declares the modes a session falls back to; without it a test can pass on
/// `default_internal_modes()` while proving nothing.
fn daemon_vm(extra: &str) -> crate::daemon_plugins::DaemonPluginLoader {
    let loader = crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
        .expect("daemon VM");
    loader
        .executor()
        .lua()
        .load(format!("{}\n{}", crucible_lua::BUILTIN_INIT_LUA, extra))
        .set_name("test defaults")
        .exec()
        .expect("the defaults file and the fixture must load");
    loader
}

/// `session.isolation` must read the same in a user's `cru.on_session_start`
/// as in a plugin's `cru.on_session_start`.
///
/// The hook runs on the daemon VM, which is the only VM that runs files. The
/// session it receives is an ARGUMENT, so one registration serves every
/// session and the field cannot read `nil` for some of them.
#[tokio::test]
async fn a_session_start_hook_sees_the_sessions_isolation_param() {
    let tmp = TempDir::new().unwrap();
    let vm = daemon_vm(
        r#"
        cru.on_session_start(function(session)
          seen_isolation = session.isolation
          seen_workspace = session.workspace
        end)
        "#,
    );

    let session_manager = temp_session_manager();
    let mut session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            Some(tmp.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    // The same second write `session.create` does for the isolation opt-in.
    session.isolation = Some(serde_json::json!("rust"));
    session_manager.update_session(&session).await.unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager.set_plugin_handlers(vm.plugin_handlers(), vm.plugin_lua());
    let _state = agent_manager.get_or_create_session_state(&session.id);

    assert_eq!(
        vm.plugin_lua()
            .globals()
            .get::<String>("seen_isolation")
            .ok(),
        Some("rust".to_string()),
        "the isolation param must reach a session's own start hook"
    );
    assert_eq!(
        vm.plugin_lua()
            .globals()
            .get::<String>("seen_workspace")
            .ok(),
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
    let vm = daemon_vm(
        r#"
        cru.on_session_start(function(session)
          seen_before = session:get_variable("visits")
          session:set_variable("visits", (seen_before or 0) + 1)
          session:set_variable("shape", { nested = true })
          seen_after = session:get_variable("visits")
        end)
        "#,
    );

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
    agent_manager.set_plugin_handlers(vm.plugin_handlers(), vm.plugin_lua());
    {
        let _state = agent_manager.get_or_create_session_state(&session.id);
        assert!(
            vm.plugin_lua()
                .globals()
                .get::<mlua::Value>("seen_before")
                .unwrap()
                .is_nil(),
            "a fresh session has no variables"
        );
        assert_eq!(
            vm.plugin_lua().globals().get::<i64>("seen_after").ok(),
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
    second.set_plugin_handlers(vm.plugin_handlers(), vm.plugin_lua());
    let _state = second.get_or_create_session_state(&session.id);
    assert_eq!(
        vm.plugin_lua().globals().get::<i64>("seen_before").ok(),
        Some(1),
        "the resumed session's hook reads the persisted variable"
    );
    assert_eq!(
        vm.plugin_lua().globals().get::<i64>("seen_after").ok(),
        Some(2)
    );
}
