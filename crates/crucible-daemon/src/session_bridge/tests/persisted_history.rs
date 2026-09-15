use super::*;

#[tokio::test]
async fn lua_can_read_ended_sessions_after_restart_without_reviving_them() {
    let sm = temp_session_manager();
    let workspace = TempDir::new().unwrap();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    sm.storage()
        .append_event(
            &session,
            &crate::observe::LogEvent::user("remember me")
                .to_jsonl()
                .unwrap(),
        )
        .await
        .unwrap();
    sm.end_session(&session.id).await.unwrap();
    let restarted = Arc::new(SessionManager::with_storage(sm.storage().clone()));
    let am = build_test_agent_manager(restarted.clone());
    let (tx, _) = broadcast::channel(32);
    let bridge = DaemonSessionBridge::new(bridge_ctx(restarted.clone(), am, tx, workspace.path()));
    let listed = bridge.list_sessions().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], session.id.as_str());
    assert_eq!(listed[0]["state"], "ended");
    let read = bridge
        .get_session(session.id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read["state"], "ended");
    let messages = bridge
        .load_messages(session.id.to_string(), None, None, false)
        .await
        .unwrap();
    assert_eq!(messages[0]["content"], "remember me");
    assert!(
        restarted.list_sessions().is_empty(),
        "reading must not revive sessions or run hooks"
    );
}
