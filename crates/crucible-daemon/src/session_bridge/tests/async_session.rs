use super::*;
use crate::delegation::DelegationSpawner;
use crate::test_support::{MockSubagentBehavior, MockSubagentHandle};
use crucible_lua::register_sessions_module_with_api;

fn rig() -> (TempDir, Arc<DaemonSessionBridge>, mlua::Lua) {
    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager();
    let (tx, _) = broadcast::channel(64);
    let am = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: sm.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(tx.clone())),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            card_roots: Default::default(),
            review_snapshot_root: tmp.path().join("snapshots"),
        },
        crate::delegation::DelegationService::new(sm.clone(), tx.clone()),
    ));
    let bridge = Arc::new(DaemonSessionBridge::new(bridge_ctx(sm, am, tx, tmp.path())));
    let lua = mlua::Lua::new();
    register_sessions_module_with_api(&lua, bridge.clone()).unwrap();
    (tmp, bridge, lua)
}

fn install_scripted_agents(bridge: &DaemonSessionBridge) -> Arc<std::sync::atomic::AtomicUsize> {
    let announced = std::sync::Mutex::new(bridge.event_tx.subscribe());
    let manager = Arc::downgrade(&bridge.agent_manager);
    let observed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = observed.clone();
    bridge
        .agent_manager
        .delegation_service()
        .bind_agent_manager(&bridge.agent_manager);
    bridge
        .agent_manager
        .set_agent_factory_override(Box::new(move |config, _| {
            // Provider setup is after the spawn announcement but before the
            // send returns. An announced child must already be collectable.
            while let Ok(event) = announced.lock().unwrap().try_recv() {
                if event.event == "delegation_spawned" {
                    counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let id = event.data["delegation_id"].as_str().unwrap();
                    assert!(
                        manager
                            .upgrade()
                            .unwrap()
                            .delegation_service()
                            .get_delegation_result(id)
                            .is_some(),
                        "announced delegation {id} was not registered"
                    );
                }
            }
            let behavior = match config.model.as_str() {
                "failure" => MockSubagentBehavior::StreamFailure("scripted failure".into()),
                "pending" => MockSubagentBehavior::Pending,
                _ => MockSubagentBehavior::ImmediateSuccess("scripted answer".into()),
            };
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(behavior))
                    as Box<
                        dyn crucible_core::traits::chat::AgentHandle + Send + Sync,
                    >)
            })
        }));
    observed
}

#[tokio::test]
async fn lua_fork_refuses_requested_or_claimed_isolation_without_creating_a_child() {
    let (tmp, bridge, lua) = rig();
    let mut parent = bridge
        .session_manager
        .create_session(SessionType::Chat, vec![], Some(tmp.path().into()), None)
        .await
        .unwrap();
    bridge
        .agent_manager
        .configure_agent(&parent.id, make_test_agent(None))
        .await
        .unwrap();
    parent = bridge.session_manager.get_session(&parent.id).unwrap();
    lua.globals().set("parent", parent.id.to_string()).unwrap();
    let before = bridge.session_manager.list_sessions().len();
    let registry = crucible_lua::IsolationRegistry::new();
    bridge.agent_manager.set_isolation(registry.clone());
    for claimed in [false, true] {
        if claimed {
            parent.isolation = None;
            registry.claim(
                &parent.id,
                crucible_lua::IsolationClaim {
                    plugin: "fixture".into(),
                    exempt: Default::default(),
                    exec: Default::default(),
                },
            );
        } else {
            parent.isolation = Some(serde_json::json!(true));
        }
        bridge
            .session_manager
            .update_session(&parent)
            .await
            .unwrap();
        lua.load(
            r#"
            local child, err = cru.session.fork(parent)
            assert(child == nil and string.find(err, "isolated"), tostring(err))
        "#,
        )
        .exec_async()
        .await
        .unwrap();
        assert_eq!(bridge.session_manager.list_sessions().len(), before);
        assert_eq!(
            std::fs::read_dir(bridge.session_manager.sessions_root())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().unwrap().is_dir())
                .count(),
            before
        );
    }
    // Ending a session retains its resident metadata but releases its claim.
    bridge
        .session_manager
        .end_session(&parent.id)
        .await
        .unwrap();
    registry.release(&parent.id);
    lua.load(
        r#"
        local child, err = cru.session.fork(parent)
        assert(child == nil and string.find(err, "isolated"), tostring(err))
    "#,
    )
    .exec_async()
    .await
    .unwrap();
    assert_eq!(bridge.session_manager.list_sessions().len(), before);
    // After restart an implicit project sandbox no longer has a live claim.
    // Only an explicit operator opt-out lets this workspace fork skip hooks.
    let cold = Arc::new(SessionManager::with_storage(
        bridge.session_manager.storage().clone(),
    ));
    let am = build_test_agent_manager(cold.clone());
    let (tx, _) = broadcast::channel(16);
    let restarted = Arc::new(DaemonSessionBridge::new(bridge_ctx(
        cold.clone(),
        am,
        tx,
        tmp.path(),
    )));
    register_sessions_module_with_api(&lua, restarted).unwrap();
    lua.load(
        r#"
        local child, err = cru.session.fork(parent)
        assert(child == nil and string.find(err, "isolated"), tostring(err))
    "#,
    )
    .exec_async()
    .await
    .unwrap();
    assert!(cold.list_sessions().is_empty());
    assert_eq!(
        cold.list_sessions_filtered_async(
            crate::session_manager::KilnFilter::Any,
            None,
            None,
            None,
            true
        )
        .await
        .len(),
        before
    );
}

#[tokio::test]
async fn lua_collection_covers_child_outcomes_and_mixed_job_ids() {
    let (tmp, bridge, lua) = rig();
    let announced = install_scripted_agents(&bridge);
    let parent = bridge
        .session_manager
        .create_session(SessionType::Chat, vec![], Some(tmp.path().into()), None)
        .await
        .unwrap();
    lua.globals().set("parent", parent.id.to_string()).unwrap();
    let current = crucible_lua::session_api::CurrentSession::new();
    current.set_current(crucible_lua::session_api::Session::new(
        parent.id.to_string(),
    ));
    crucible_lua::register_sessions_module_with_api_and_current(&lua, bridge.clone(), current)
        .unwrap();
    for (model, expected) in [
        ("success", "completed"),
        ("failure", "failed"),
        ("pending", "timeout"),
    ] {
        let mut config = make_test_agent(None);
        config.model = model.into();
        config.delegation_config = Some(crucible_core::config::DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
            timeout_secs: 300,
        });
        bridge
            .agent_manager
            .configure_agent(&parent.id, config)
            .await
            .unwrap();
        lua.globals().set("expected", expected).unwrap();
        lua.globals()
            .set("wait_seconds", if model == "pending" { 0.0 } else { 2.0 })
            .unwrap();
        let bash = bridge
            .agent_manager
            .background_manager()
            .spawn_bash(
                &parent.id,
                "printf bash-answer".into(),
                Some(tmp.path().into()),
                None,
            )
            .await
            .unwrap();
        lua.globals().set("bash_job", bash).unwrap();
        tokio::time::timeout(Duration::from_secs(5), lua.load(r#"
            local job, err = cru.session.create({ delegate = true, parent_session_id = parent, prompt = "child task" })
            assert(job ~= nil, tostring(err))
            child = job.delegation_id
            local rows, collect_err = cru.session.collect_subagents({child, bash_job, child, "unknown"}, wait_seconds)
            assert(collect_err == nil, tostring(collect_err))
            assert(#rows == 4 and rows[1].id == child and rows[3].id == child)
            assert(rows[1].status == expected, rows[1].status)
            assert(rows[3].status == expected and rows[4].status == "not_found")
            if expected == "completed" then
                assert(rows[1].output == "scripted answer")
                assert(rows[2].status == "completed" and rows[2].output == "bash-answer")
            elseif expected == "failed" then
                assert(string.find(rows[1].error, "scripted failure"))
            end
        "#).exec_async()).await.unwrap().unwrap();
        if model == "pending" {
            let child: String = lua.globals().get("child").unwrap();
            let service = bridge.agent_manager.delegation_service();
            assert!(
                !service
                    .get_delegation_result(&child)
                    .unwrap()
                    .info
                    .status
                    .is_terminal(),
                "collection timeout must not cancel work"
            );
            assert!(service.cancel_delegation(&child).await);
            tokio::time::timeout(
                Duration::from_secs(5),
                lua.load(
                    r#"
                local rows = assert(cru.session.collect_subagents({child}, 2))
                assert(rows[1].status == "cancelled", rows[1].status)
            "#,
                )
                .exec_async(),
            )
            .await
            .unwrap()
            .unwrap();
        }
    }
    assert_eq!(announced.load(std::sync::atomic::Ordering::Relaxed), 3);
}

#[tokio::test]
async fn job_completion_wakes_all_collectors_without_advancing_a_poll_clock() {
    let (_tmp, bridge, _lua) = rig();
    let manager = &bridge.agent_manager;
    let job = manager
        .background_manager()
        .spawn_bash("parent", "sleep 60".into(), None, None)
        .await
        .unwrap();
    tokio::time::pause();
    let jobs = [job.clone()];
    let first = manager.collect_jobs(&jobs, Duration::MAX);
    let second = manager.collect_jobs(&jobs, Duration::from_secs(2));
    tokio::pin!(first, second);
    assert!(futures::poll!(&mut first).is_pending());
    assert!(futures::poll!(&mut second).is_pending());
    assert!(manager.background_manager().cancel_job(&job).await);
    for ready in [futures::poll!(&mut first), futures::poll!(&mut second)] {
        let std::task::Poll::Ready(rows) = ready else {
            panic!("completion required a polling tick")
        };
        assert_eq!(rows[0]["status"], "cancelled");
    }
    assert_eq!(manager.background_manager().list_jobs("parent").len(), 1);
}

#[tokio::test]
async fn lua_send_and_collect_observes_success_failure_timeout_and_cancellation() {
    let (tmp, bridge, lua) = rig();
    install_scripted_agents(&bridge);
    for model in ["success", "failure", "pending", "cancel"] {
        let session = bridge
            .session_manager
            .create_session(SessionType::Chat, vec![], Some(tmp.path().into()), None)
            .await
            .unwrap();
        let mut config = make_test_agent(None);
        config.model = if model == "cancel" { "pending" } else { model }.into();
        bridge
            .agent_manager
            .configure_agent(&session.id, config)
            .await
            .unwrap();
        lua.globals()
            .set("session_id", session.id.to_string())
            .unwrap();
        lua.globals()
            .set("wait_seconds", if model == "pending" { 0.0 } else { 2.0 })
            .unwrap();
        lua.load(r#"next_part = assert(cru.session.send_and_collect(session_id, "question", {timeout = wait_seconds}))"#).exec_async().await.unwrap();
        if model == "cancel" {
            assert!(bridge.agent_manager.cancel(&session.id).await);
        }
        let text: String = tokio::time::timeout(
            Duration::from_secs(3),
            lua.load(
                r#"
            local text = ""
            while true do
                local part = next_part()
                if part == nil then return text end
                if part.type == "text" then text ..= part.content end
            end
        "#,
            )
            .eval_async(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            text,
            if model == "success" {
                "scripted answer"
            } else {
                ""
            }
        );
        if model == "pending" {
            assert!(
                bridge.agent_manager.cancel(&session.id).await,
                "timeout must stop observing, not cancel the turn"
            );
        }
    }
}

#[tokio::test]
async fn lua_unsubscribe_closes_its_iterators_without_closing_other_sessions() {
    let (_tmp, bridge, lua) = rig();
    lua.load(
        r#"
        first = assert(cru.session.subscribe("first"))
        second = assert(cru.session.subscribe("second"))
    "#,
    )
    .exec_async()
    .await
    .unwrap();
    let emit = |id, content| {
        assert!(crate::event_emitter::emit_event(
            &bridge.event_tx,
            SessionEventMessage::new(id, "text_delta", serde_json::json!({"content":content})),
        ));
    };
    emit("first", "one");
    emit("second", "two");
    tokio::time::timeout(
        Duration::from_secs(2),
        lua.load(
            r#"
        assert(first().data.content == "one")
        assert(second().data.content == "two")
        assert(cru.session.unsubscribe("first"))
        assert(cru.session.unsubscribe("first")) -- idempotent
        assert(first() == nil)
        first = assert(cru.session.subscribe("first")) -- fresh generation
    "#,
        )
        .exec_async(),
    )
    .await
    .expect("unsubscribe must end an idle iterator")
    .unwrap();
    for id in ["first", "second"] {
        emit(id, "still live");
    }
    tokio::time::timeout(
        Duration::from_secs(2),
        lua.load(
            r#"
        assert(first().data.content == "still live")
        assert(second().data.content == "still live")
        assert(cru.session.unsubscribe("first"))
        assert(cru.session.unsubscribe("second"))
    "#,
        )
        .exec_async(),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn dropping_a_subscription_releases_the_broadcast_receiver_while_idle() {
    let (_tmp, bridge, _lua) = rig();
    let baseline = bridge.event_tx.receiver_count();
    let receiver = bridge.subscribe("idle".into()).await.unwrap();
    assert_eq!(bridge.event_tx.receiver_count(), baseline + 1);
    drop(receiver);
    tokio::time::timeout(Duration::from_secs(2), async {
        while bridge.event_tx.receiver_count() != baseline {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle forwarder leaked its broadcast subscription");
}

#[tokio::test]
async fn lua_collection_rejects_invalid_timeouts_before_starting_work() {
    let (_tmp, bridge, lua) = rig();
    let sessions_before = bridge.session_manager.list_sessions().len();
    lua.load(r#"
        for _, timeout in ipairs({-1, 0/0, math.huge, -math.huge, 1e19, 1e300}) do
            local result, err = cru.session.collect_subagents({}, timeout)
            assert(result == nil and string.find(err, "timeout"), tostring(err))
            local next_part, send_err = cru.session.send_and_collect("missing", "never send", { timeout = timeout })
            assert(next_part == nil and string.find(send_err, "timeout"), tostring(send_err))
        end
        local result, err = cru.session.collect_subagents({"missing", "missing"}, 0)
        assert(err == nil and #result == 2)
        assert(result[1].status == "not_found" and result[2].status == "not_found")
    "#).exec_async().await.unwrap();
    assert_eq!(
        bridge.session_manager.list_sessions().len(),
        sessions_before
    );
}
