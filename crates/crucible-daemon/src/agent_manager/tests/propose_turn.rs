//! A turn in a mode that declares `writes = "propose"`: the note tools record
//! a proposal, the disk does not change, and the turn proposal ends with the
//! turn.

use super::*;

/// A scripted agent that reports the `propose` mode, so the turn start reads
/// that mode.
struct ProposingAgent {
    events: Vec<TurnEvent>,
}

#[async_trait::async_trait]
impl crucible_core::turn::Agent for ProposingAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<futures::stream::BoxStream<'a, TurnEvent>, crucible_core::turn::AgentError> {
        Ok(scripted_events_stream(self.events.clone(), ctx))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crucible_core::impl_unsupported_session_knobs!(ProposingAgent);

#[async_trait::async_trait]
impl AgentHandle for ProposingAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "propose"
    }
    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
}

fn propose_registry() -> crucible_lua::ModeRegistry {
    let lua = mlua::Lua::new();
    let registry = crucible_lua::ModeRegistry::new();
    crucible_lua::register_modes(&lua, registry.clone()).unwrap();
    lua.load(r#"cru.modes.propose = { permissions = "allow", writes = "propose" }"#)
        .exec()
        .unwrap();
    registry
}

#[tokio::test]
async fn a_propose_turn_leaves_the_disk_unchanged_and_lists_a_proposal() {
    let kiln = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let sm = temp_session_manager_with_kilns(&[("knowledge", kiln.path())]);
    let am = create_test_agent_manager(sm.clone()).with_modes(Some(propose_registry()));
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![kiln_name("knowledge")],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    let mut agent = test_agent();
    agent.tool_policy = Some(HashMap::from([(
        "create_note".into(),
        crucible_core::agent::ToolPolicy::Allow,
    )]));
    am.configure_agent(&session.id, agent).await.unwrap();
    am.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(ProposingAgent {
            events: vec![script::tool_call(
                "note-1",
                "create_note",
                serde_json::json!({ "path": "Lesson.md", "content": "# Lesson" }),
            )],
        }))),
    );

    let (tx, mut rx) = broadcast::channel(128);
    am.send_message(&session.id, "Remember this".into(), &tx, false, None)
        .await
        .unwrap();
    next_event_or_skip(&mut rx, "message_complete").await;
    // The turn task ends the turn proposal after the last event.
    for _ in 0..100 {
        if !am.proposals().has_turn(session.id.as_str()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        !kiln.path().join("Lesson.md").exists(),
        "a propose turn must not write the note"
    );
    let listed = am.proposals().list(false).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].writes[0].path, "Lesson.md");
    assert_eq!(listed[0].writes[0].new_text, "# Lesson");
    assert!(
        !am.proposals().has_turn(session.id.as_str()),
        "the turn proposal ends with the turn"
    );
}

/// A propose turn writes nothing to the disk, so the pre-write review gate
/// has nothing to hold. An earlier unreviewed hunk on the target must not
/// block the turn: for a plugin pass, no person answers the gate.
#[tokio::test]
async fn a_propose_turn_is_not_held_by_the_review_gate() {
    // The kiln is a git repository, so the session has a review ledger.
    let kiln = TempDir::new().unwrap();
    crate::test_support::init_repo(kiln.path(), &[("note.md", "# Old\n")]).await;
    let workspace = TempDir::new().unwrap();
    let sm = temp_session_manager_with_kilns(&[("knowledge", kiln.path())]);
    let am = create_test_agent_manager(sm.clone()).with_modes(Some(propose_registry()));
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![kiln_name("knowledge")],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();

    // An earlier write of the target, left unreviewed.
    am.review
        .open(&session.id, &[kiln.path().to_path_buf()])
        .await
        .unwrap();
    let earlier = am.review.open_bracket(&session.id).await.unwrap();
    std::fs::write(kiln.path().join("note.md"), "# Earlier\n").unwrap();
    assert!(am
        .review
        .close(&session.id, earlier, "call-earlier", 0)
        .await
        .unwrap());
    assert_eq!(
        am.review
            .has_unreviewed_in_file(&session.id, &[kiln.path().join("note.md")])
            .await
            .unwrap(),
        crucible_core::session::Verdict::Unreviewed,
        "the fixture must leave an unreviewed hunk on the target"
    );

    let mut agent = test_agent();
    agent.tool_policy = Some(HashMap::from([(
        "update_note".into(),
        crucible_core::agent::ToolPolicy::Allow,
    )]));
    am.configure_agent(&session.id, agent).await.unwrap();
    am.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(ProposingAgent {
            events: vec![script::tool_call(
                "note-1",
                "update_note",
                serde_json::json!({ "path": "note.md", "content": "# New\n" }),
            )],
        }))),
    );

    let (tx, mut rx) = broadcast::channel(256);
    let (_message_id, done) = am
        .send_message_notified(&session.id, "Tidy the note".into(), &tx, false, None)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), done)
        .await
        .expect("the review gate held a propose turn")
        .expect("turn outcome");

    while let Ok(msg) = rx.try_recv() {
        assert!(
            !(msg.event == "review_gate" && msg.data["blocked"] == serde_json::json!(true)),
            "a propose turn must not wait on review: {msg:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(kiln.path().join("note.md")).unwrap(),
        "# Earlier\n"
    );
    let listed = am.proposals().list(false).unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].writes[0].new_text, "# New\n");
}
