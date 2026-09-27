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
    let am = Arc::new(
        Arc::into_inner(create_test_agent_manager(sm.clone()))
            .expect("the fixture hands out the only reference")
            .with_modes(Some(propose_registry())),
    );
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

    let (tx, mut rx) = crate::EventBus::channel(128);
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
