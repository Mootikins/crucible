//! The plugin create surface: `cru.sessions.create` runs the daemon's real
//! create path, so everything the RPC handler does — scope refusal, trust
//! validation, agent-card resolution, the setup task — happens here too.
//!
//! Split out of `tests/mod.rs` because that file reached the 1000-line module
//! budget; the fixtures it shares still live there.

use super::*;

const RESEARCHER_CARD: &str =
    "---\nname: researcher\ndescription: Explores and synthesizes\nmodel: llama3.2\n---\n\nYou are a researcher.\n";

/// A bridge over a fresh session manager, with `data_home` = `kiln`.
///
/// The receiver comes back with the bridge because the create path's setup
/// task broadcasts through the context's sender: subscribing after `create`
/// returns would race the task, and there is no synchronous point to await.
fn create_rig_with_llm_config(
    kiln: &std::path::Path,
    llm_config: LlmConfig,
) -> (
    Arc<SessionManager>,
    DaemonSessionBridge,
    broadcast::Receiver<SessionEventMessage>,
) {
    let session_manager = Arc::new(SessionManager::with_storage(Arc::new(
        FileSessionStorage::new(),
    )));
    let agent_manager =
        build_test_agent_manager_with_llm_config(session_manager.clone(), Some(llm_config.clone()));
    let (event_tx, events) = broadcast::channel(256);
    let bridge = DaemonSessionBridge::new(bridge_ctx_with_llm_config(
        session_manager.clone(),
        agent_manager,
        event_tx,
        kiln,
        Some(llm_config),
    ));
    (session_manager, bridge, events)
}

fn create_rig(kiln: &std::path::Path) -> (Arc<SessionManager>, DaemonSessionBridge) {
    let (session_manager, bridge, _events) = create_rig_with_llm_config(kiln, bridge_llm_config());
    (session_manager, bridge)
}

/// The reason the trait widened to a params object: a plugin can now name an
/// agent card, which the old four-scalar create had no way to express.
#[tokio::test]
async fn bridge_create_resolves_an_agent_card_from_the_kiln() {
    let tmp = TempDir::new().unwrap();
    let cards = tmp.path().join(".crucible").join("agents");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(cards.join("researcher.md"), RESEARCHER_CARD).unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    let created = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
            "configure_agent": true,
            "agent_card": "researcher",
        }))
        .await
        .expect("bridge create with an agent card");

    let session = session_manager
        .get_session(created["id"].as_str().unwrap())
        .expect("session registered");
    let agent = session.agent.expect("create configured the agent");
    assert_eq!(agent.agent_card_name.as_deref(), Some("researcher"));
    assert_eq!(agent.model, "llama3.2");
    assert_eq!(agent.system_prompt, "You are a researcher.");
    // A card is an internal agent, never an ACP profile: a set `agent_name` is
    // what forces `TrustLevel::Cloud` at runtime.
    assert_eq!(agent.agent_name, None);
}

/// A card may name a *specialty* instead of a model, mapped through
/// `[llm.models]`. The mapping reads the context's `llm_config`, so a fixture
/// without one answers with the base model and looks like it worked.
#[tokio::test]
async fn bridge_create_maps_a_cards_specialty_through_llm_models() {
    let tmp = TempDir::new().unwrap();
    let cards = tmp.path().join(".crucible").join("agents");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(
        cards.join("digger.md"),
        "---\nname: digger\ndescription: Digs\nspecialty: research\n---\n\nYou dig.\n",
    )
    .unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    let created = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
            "configure_agent": true,
            "agent_card": "digger",
        }))
        .await
        .expect("bridge create with a specialty card");

    let agent = session_manager
        .get_session(created["id"].as_str().unwrap())
        .unwrap()
        .agent
        .expect("create configured the agent");
    // `ollama/llama3.2-deep` splits on the backend prefix, so the model is the
    // remainder and the provider is the named backend.
    assert_eq!(agent.model, "llama3.2-deep");
    assert_eq!(agent.provider, BackendType::Ollama);
}

/// A create-time `tool_policy` is the caller's per-session decision and beats
/// the card's own `tools:` block.
///
/// This is what lets a caller use a card at all without walking the card back:
/// `configure_agent` replaces the whole agent, so setting a tool policy
/// afterwards would discard the card's prompt and model.
#[tokio::test]
async fn bridge_create_tool_policy_overrides_the_cards_own_tools() {
    use crucible_core::agent::ToolPolicy;

    let tmp = TempDir::new().unwrap();
    let cards = tmp.path().join(".crucible").join("agents");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(
        cards.join("loose.md"),
        "---\nname: loose\ndescription: Wide open\nmodel: llama3.2\ntools:\n  bash: allow\n---\n\nYou are loose.\n",
    )
    .unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    let created = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
            "configure_agent": true,
            "agent_card": "loose",
            "tool_policy": { "bash": "deny", "read_file": "allow" },
        }))
        .await
        .expect("bridge create with a tool policy");

    let agent = session_manager
        .get_session(created["id"].as_str().unwrap())
        .unwrap()
        .agent
        .expect("create configured the agent");
    let policy = agent.tool_policy.expect("policy reached the agent");
    assert_eq!(policy.get("bash"), Some(&ToolPolicy::Deny));
    assert_eq!(policy.get("read_file"), Some(&ToolPolicy::Allow));
    // The rest of the card survived the override.
    assert_eq!(agent.system_prompt, "You are loose.");
    assert_eq!(agent.agent_card_name.as_deref(), Some("loose"));
}

/// The agent is resolved before the session exists, so a typo leaves nothing
/// behind — and the error names what was available.
///
/// A session already exists when the bad create runs, so "nothing was created"
/// is checked as *the list did not change* rather than as "the list is empty":
/// an ordering regression that created the session before resolving the agent
/// would leave an agent-less row behind, which an emptiness assertion on a
/// fresh manager would also catch but a real daemon never sees.
#[tokio::test]
async fn bridge_create_with_an_unknown_card_errors_and_creates_no_session() {
    let tmp = TempDir::new().unwrap();
    let cards = tmp.path().join(".crucible").join("agents");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(cards.join("researcher.md"), RESEARCHER_CARD).unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
        }))
        .await
        .expect("a plain create to have something for the bad one to disturb");
    let before: Vec<String> = session_manager
        .list_sessions()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(before.len(), 1);

    let err = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
            "configure_agent": true,
            "agent_card": "nope",
        }))
        .await
        .expect_err("an unknown card must fail");

    assert!(err.contains("Unknown agent card: nope"), "got: {err}");
    assert!(
        err.contains("researcher"),
        "error must list what exists: {err}"
    );
    let after: Vec<String> = session_manager
        .list_sessions()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(after, before, "a refused create must register no session");
}

/// A kiln-less plugin create resolves to the daemon's own data root.
///
/// The bridge must send `kiln: None` rather than pre-resolving
/// `crucible_home()`: a path the bridge invented would be scope-checked as if
/// the caller had asked for it, which fails outright whenever the data root is
/// `$HOME` — and it would ignore an injected root, as this test's tempdir is.
#[tokio::test]
async fn bridge_create_without_a_kiln_lands_in_the_daemons_data_root() {
    let tmp = TempDir::new().unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    let created = bridge
        .create_session(serde_json::json!({ "type": "chat" }))
        .await
        .expect("kiln-less bridge create");

    assert_eq!(
        created["kiln"].as_str(),
        Some(&*tmp.path().to_string_lossy())
    );
    let session = session_manager
        .get_session(created["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(session.kiln, tmp.path());
}

/// The bridge deserializes with the same request type the RPC handler uses, so
/// the mutual-exclusion rule holds on the plugin surface too.
#[tokio::test]
async fn bridge_create_rejects_agent_card_and_agent_name_together() {
    let tmp = TempDir::new().unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    let err = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "configure_agent": true,
            "agent_card": "researcher",
            "agent_name": "claude",
        }))
        .await
        .expect_err("both agent fields must be refused");

    assert!(err.contains("mutually exclusive"), "got: {err}");
    assert!(session_manager.list_sessions().is_empty());
}

/// `kilns` is the plugin spelling; `connect_kilns` is the wire name. The alias
/// is the only thing keeping a plugin's read kilns from vanishing silently.
#[tokio::test]
async fn bridge_create_accepts_the_lua_kilns_spelling() {
    let tmp = TempDir::new().unwrap();
    let other = tmp.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let (session_manager, bridge) = create_rig(tmp.path());

    let created = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
            "kilns": [other.to_string_lossy()],
        }))
        .await
        .expect("bridge create with kilns");

    let session = session_manager
        .get_session(created["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(session.connected_kilns, vec![other]);
}

/// A plugin-created session announces itself exactly like an RPC-created one.
///
/// `session_initialized` is the first thing the setup task emits and the only
/// signal a subscriber gets that a session exists; a bridge that created
/// sessions without spawning the task would leave every event consumer — the
/// TUI, the web client, the recording writer — blind to plugin sessions.
///
/// The `model` assertion is the second half: the agent is configured *before*
/// the task is spawned, so the announcement carries the real model rather than
/// the empty string an agent-less session emits.
#[tokio::test]
async fn bridge_create_emits_session_initialized() {
    let tmp = TempDir::new().unwrap();
    let cards = tmp.path().join(".crucible").join("agents");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(cards.join("researcher.md"), RESEARCHER_CARD).unwrap();
    let (_session_manager, bridge, mut events) =
        create_rig_with_llm_config(tmp.path(), bridge_llm_config());

    let created = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": tmp.path().to_string_lossy(),
            "configure_agent": true,
            "agent_card": "researcher",
        }))
        .await
        .expect("bridge create");
    let session_id = created["id"].as_str().unwrap().to_string();

    // Drain rather than sleep: the setup task also indexes and discovers, so
    // other events may land first, and how long any of it takes is not this
    // test's business.
    let mut initialized = None;
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_secs(10), events.recv()).await {
        if event.event == "session_initialized" {
            initialized = Some(event);
            break;
        }
    }

    let event = initialized.expect("the setup task must announce the session");
    assert_eq!(event.session_id, session_id);
    assert_eq!(
        event.data["kiln_path"].as_str(),
        Some(&*tmp.path().to_string_lossy())
    );
    assert_eq!(
        event.data["model"].as_str(),
        Some("llama3.2"),
        "the announcement must carry the agent create resolved: {:?}",
        event.data
    );
}

/// A workspace whose only kiln is classified `confidential`, holding
/// `researcher.md` in the kiln's own card directory.
///
/// The provider is explicitly `Local` — the one trust level a confidential
/// kiln clears — so what the tests below vary is the *kind* of agent, not the
/// provider behind it.
fn confidential_card_kiln(tmp: &TempDir) -> (PathBuf, PathBuf, LlmConfig) {
    let workspace = tmp.path().join("ws");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(kiln.join(".crucible").join("agents")).unwrap();
    std::fs::create_dir_all(workspace.join(".crucible")).unwrap();
    std::fs::write(
        workspace.join(".crucible").join("project.toml"),
        "[[kilns]]\npath = \"./notes\"\ndata_classification = \"confidential\"\n",
    )
    .unwrap();
    std::fs::write(
        kiln.join(".crucible").join("agents").join("researcher.md"),
        RESEARCHER_CARD,
    )
    .unwrap();
    let llm_config = crate::test_fixtures::build_llm_config_with_trust(
        "local",
        BackendType::Ollama,
        Some(crucible_core::config::TrustLevel::Local),
    );
    (workspace, kiln, llm_config)
}

/// A card session's trust is its provider's, so a card on a local provider is
/// admitted to a confidential kiln.
///
/// Both trust gates have to agree for this to pass: the create-time one
/// (`resolve_provider_trust_level_for_create`) and the runtime one
/// (`resolve_provider_trust`, reached through `AgentManager::configure_agent`).
/// They now share a discriminator — `agent_type` — and a card is an internal
/// agent whichever gate is asking.
///
/// What this pins is the end-to-end result, not the discriminator itself:
/// `SessionAgent::from_card` clears `agent_name`, so a card session was already
/// not-Cloud under the old `agent_name.is_some()` rule. The test that isolates
/// the discriminator is
/// `trust_resolution::tests::provider_trust_follows_the_provider_not_the_agent_name`;
/// its counterpart from the other side is
/// [`an_acp_profile_session_is_refused_the_kiln_a_card_session_clears`], below.
#[tokio::test]
async fn a_card_session_is_trusted_at_its_providers_level() {
    use crucible_core::config::{DataClassification, TrustLevel};

    let tmp = TempDir::new().unwrap();
    let (workspace, kiln, llm_config) = confidential_card_kiln(&tmp);
    let (session_manager, bridge, _events) = create_rig_with_llm_config(&kiln, llm_config.clone());

    let created = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": kiln.to_string_lossy(),
            "workspace": workspace.to_string_lossy(),
            "configure_agent": true,
            "agent_card": "researcher",
        }))
        .await
        .expect("a local provider clears a confidential kiln");

    let agent = session_manager
        .get_session(created["id"].as_str().unwrap())
        .unwrap()
        .agent
        .expect("create configured the agent");
    assert_eq!(agent.agent_card_name.as_deref(), Some("researcher"));
    assert_eq!(agent.agent_type, "internal");
    assert_eq!(agent.provider_key.as_deref(), Some("local"));

    let trust = crate::trust_resolution::resolve_provider_trust(&agent, Some(&llm_config));
    assert_eq!(trust, TrustLevel::Local);
    assert!(
        trust.satisfies(DataClassification::Confidential),
        "the consequence of resolving Local: the kiln's notes stay reachable"
    );
}

/// The other side of the same discriminator: an ACP profile on the same kiln,
/// with the same local provider configured, is refused.
///
/// An external agent process picks its own model, so the daemon cannot vouch
/// for where the prompt lands — `agent_type == "acp"` is Cloud regardless of
/// what `[llm.providers]` says, and Cloud does not clear confidential.
#[tokio::test]
async fn an_acp_profile_session_is_refused_the_kiln_a_card_session_clears() {
    let tmp = TempDir::new().unwrap();
    let (workspace, kiln, llm_config) = confidential_card_kiln(&tmp);
    let (session_manager, bridge, _events) = create_rig_with_llm_config(&kiln, llm_config);

    let err = bridge
        .create_session(serde_json::json!({
            "type": "chat",
            "kiln": kiln.to_string_lossy(),
            "workspace": workspace.to_string_lossy(),
            "configure_agent": true,
            "agent_type": "acp",
            "agent_name": "claude",
        }))
        .await
        .expect_err("an ACP agent is always Cloud, whatever the provider config says");

    assert!(err.contains("insufficient"), "got: {err}");
    assert!(err.contains("confidential"), "got: {err}");
    assert!(
        session_manager.list_sessions().is_empty(),
        "a refused create must register no session"
    );
}

/// `cru.sessions.configure_agent` is the other half of the plugin create
/// surface, and it is the step that would otherwise walk a session onto a
/// provider its kiln never cleared. Gated in `AgentManager::configure_agent`,
/// so the plugin door and the RPC door cannot answer differently.
#[tokio::test]
async fn bridge_configure_agent_refuses_a_provider_the_attached_kiln_does_not_clear() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("ws");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    std::fs::create_dir_all(workspace.join(".crucible")).unwrap();
    std::fs::write(
        workspace.join(".crucible").join("project.toml"),
        "[[kilns]]\npath = \"./notes\"\ndata_classification = \"confidential\"\n",
    )
    .unwrap();

    let session_manager = Arc::new(SessionManager::new());
    let session = session_manager
        .create_session(SessionType::Chat, kiln, None, vec![], None)
        .await
        .unwrap();
    let agent_manager = build_test_agent_manager(session_manager.clone());
    let (event_tx, _) = broadcast::channel(16);
    let bridge = DaemonSessionBridge::new(bridge_ctx(
        session_manager.clone(),
        agent_manager,
        event_tx,
        tmp.path(),
    ));

    // No llm_config on this manager, so any provider resolves to Cloud — which
    // a Confidential kiln does not clear.
    let err = bridge
        .configure_agent(
            session.id.clone(),
            serde_json::to_value(make_test_agent(None)).unwrap(),
        )
        .await
        .expect_err("the plugin surface must be gated too");
    assert!(
        err.contains("insufficient for the attached kiln"),
        "got: {err}"
    );
    assert!(session_manager
        .get_session(&session.id)
        .unwrap()
        .agent
        .is_none());
}
