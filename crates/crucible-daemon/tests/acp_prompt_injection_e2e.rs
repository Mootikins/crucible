//! Daemon-side context injection reaching an ACP agent's wire prompt.
//!
//! An ACP agent owns its history, so the daemon sends it only the new turn's
//! content. Everything the daemon knows and the agent does not — the kiln
//! context Precognition retrieves, a block a Lua plugin adds — has to travel
//! as part of that one prompt or it does not travel at all.
//!
//! The seam is `acp_prompt_text`, and it is unit-tested in
//! `acp_handle/translate.rs`. `acp_smoke.rs` tests one layer up, but hands
//! `turn()` a `TurnContext` it built itself, with a System block it wrote by
//! hand. Both stop short of the same thing: nothing drives the *daemon* into
//! computing an injection and then checks what the agent process received.
//!
//! So the chain from `compute_precognition_message` through `StreamContext`,
//! `apply_transform_context_handlers`, `ctx.messages`, `acp_prompt_text` and
//! the ACP wire has no test that crosses it. Every stage of it is covered
//! against an in-process mock agent, which is not where it breaks: the ACP
//! handle is the one consumer that must re-linearize the message array
//! instead of passing it through.
//!
//! These tests read the file the agent process wrote
//! (`CRU_MOCK_PROMPT_CAPTURE`), so the assertions are on bytes that crossed
//! a process boundary, not on a struct the test also built.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crucible_core::config::{AcpConfig, AgentProfile, BackendType, EmbeddingProviderConfig};
use crucible_core::session::{SessionAgent, SessionType};
use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::{AgentManager, AgentManagerParams, BackgroundJobManager, KilnManager};
use crucible_lua::PluginSource;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::mock_agent_path;

/// A cold spawn, a handshake, a retrieval pass and a turn.
const TURN_TIMEOUT: Duration = Duration::from_secs(90);

/// The note the kiln holds. The title is distinctive so its presence in the
/// captured prompt cannot be an accident of the user's own words.
const NOTE_TITLE: &str = "Widget Calibration";
const NOTE_FILE: &str = "Widget Calibration.md";
const NOTE_BODY: &str = "---\ntitle: Widget Calibration\ntags:\n  - widgets\n---\n\n\
     A widget is calibrated by setting its tolerance to 0.4 millimetres.\n";

/// What the user types. Deliberately free of the note's title, so the title
/// can only reach the wire by way of the injection.
const USER_MESSAGE: &str = "How do I set the tolerance?";

/// The mock's embedding width, matching `EmbeddingProviderConfig::mock`.
const EMBEDDING_DIMENSIONS: u32 = 384;

/// The ACP profile the session runs, capturing its prompt to `capture`.
fn capturing_profile(capture: &Path) -> AgentProfile {
    let mut env = BTreeMap::new();
    env.insert(
        "CRU_MOCK_PROMPT_CAPTURE".to_string(),
        capture.to_string_lossy().into_owned(),
    );
    env.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "acknowledged".to_string(),
    );
    AgentProfile {
        extends: None,
        command: Some(mock_agent_path().to_string_lossy().into_owned()),
        args: Some(Vec::new()),
        env,
        description: Some("mock ACP agent for injection tests".to_string()),
        delegation: None,
        permissions: None,
    }
}

fn acp_agent(precognition_enabled: bool) -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("mock-acp".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "mock-acp".to_string(),
        system_prompt: String::new(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
        mode: None,
    }
}

/// A one-file plugin whose body is `init`, loaded by the real loader so its
/// handlers reach the turn through the production path.
async fn load_plugin(root: &Path, init: &str) -> DaemonPluginLoader {
    let plugins = root.join("plugins");
    let dir = plugins.join("injector");
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(dir.join("init.lua"), init).expect("init.lua");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(plugins, PluginSource::EnvPath)])
        .await
        .expect("load plugins");
    loader
}

/// Everything a turn needs, plus the file the agent process writes its
/// received prompt to.
struct Harness {
    _temp: TempDir,
    capture_path: PathBuf,
    agent_manager: Arc<AgentManager>,
    session_id: crucible_core::session::SessionId,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

/// A session over a kiln holding one indexed note, whose agent is the ACP
/// mock. `plugin_init`, when given, is loaded as a real plugin.
async fn setup(precognition_enabled: bool, plugin_init: Option<&str>) -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let kiln_path = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).expect("kiln dir");
    std::fs::write(kiln_path.join(NOTE_FILE), NOTE_BODY).expect("write note");
    let capture_path = temp.path().join("captured_prompt.txt");

    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln_path)]);
    let (event_tx, _event_rx) = broadcast::channel(256);

    let loaded_plugin = match plugin_init {
        Some(init) => Some(load_plugin(temp.path(), init).await),
        None => None,
    };
    // What `server/plugin_boot.rs` does at startup. Without it a plugin is
    // loaded but its turn-loop handlers are never folded into a turn.
    let plugin_handlers = loaded_plugin
        .as_ref()
        .map(|loader| (loader.plugin_handlers(), loader.plugin_lua()));
    let plugin_loader = Arc::new(tokio::sync::Mutex::new(loaded_plugin));

    // Held here as well as in the manager: `AgentManager::kiln_manager` is
    // private, and the note has to be indexed through the same instance the
    // turn will retrieve from.
    let kiln_manager = Arc::new(KilnManager::with_event_tx(
        event_tx.clone(),
        Some(EmbeddingProviderConfig::mock(Some(EMBEDDING_DIMENSIONS))),
        crucible_core::config::default_max_precognition_chars(),
    ));

    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: kiln_manager.clone(),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: Some(AcpConfig {
            default_agent: None,
            streaming_timeout_minutes: 1,
            agents: BTreeMap::from([("mock-acp".to_string(), capturing_profile(&capture_path))]),
        }),
        context_config: None,
        permission_config: None,
        plugin_loader: Some(plugin_loader),
        card_roots: Default::default(),
    }));

    if let Some((registry, lua)) = plugin_handlers {
        agent_manager.set_plugin_handlers(registry, lua);
    }

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![kiln_name("kiln")],
            Some(kiln_path.clone()),
            None,
        )
        .await
        .expect("session");

    // Index the note so retrieval has something to find. The mock embedding
    // provider answers every query with the same vector, so a single stored
    // embedding of the same width is a guaranteed match.
    let handle = kiln_manager
        .get_or_open(&kiln_path)
        .await
        .expect("open kiln");
    handle
        .as_note_store()
        .upsert(
            crucible_core::storage::note_store::NoteRecord::new(
                NOTE_FILE,
                crucible_core::parser::BlockHash::zero(),
            )
            .with_title(NOTE_TITLE)
            .with_embedding(vec![0.1; EMBEDDING_DIMENSIONS as usize])
            .with_embedding_metadata("mock-model".to_string(), EMBEDDING_DIMENSIONS),
        )
        .await
        .expect("index the note");

    agent_manager
        .configure_agent(&session.id, acp_agent(precognition_enabled))
        .await
        .expect("configure the ACP agent");

    Harness {
        _temp: temp,
        capture_path,
        agent_manager,
        session_id: session.id,
        event_tx,
    }
}

/// Run one turn and return the prompt text the agent process received.
async fn prompt_seen_by_the_agent(h: &Harness) -> String {
    let (_message_id, done) = h
        .agent_manager
        .send_message_notified(
            &h.session_id,
            USER_MESSAGE.to_string(),
            &h.event_tx,
            true,
            None,
        )
        .await
        .expect("the turn is accepted");
    let _ = timeout(TURN_TIMEOUT, done)
        .await
        .expect("the turn finished");

    std::fs::read_to_string(&h.capture_path)
        .expect("the agent process must have captured the prompt it received")
}

/// The headline: kiln context the daemon retrieved reaches the external
/// agent's prompt. Without this the ACP agent answers from the user's words
/// alone and the kiln might as well not exist.
#[tokio::test]
async fn daemon_computed_precognition_reaches_the_acp_wire_prompt() {
    let h = setup(true, None).await;
    let prompt = prompt_seen_by_the_agent(&h).await;

    assert!(
        prompt.contains(NOTE_TITLE),
        "the retrieved note must reach the ACP prompt; the agent received: {prompt:?}"
    );
    assert!(
        prompt.contains(USER_MESSAGE),
        "the user's own message must still reach the ACP prompt; \
         the agent received: {prompt:?}"
    );
}

/// The control. Without it the test above would also pass if the note title
/// arrived by some route that has nothing to do with Precognition — the
/// session's kiln path, say, or a system prompt that lists the kiln.
///
/// It is also the honest reading of the assertion above: what makes the
/// title appear is the injection, and turning the injection off removes it.
#[tokio::test]
async fn with_precognition_off_no_kiln_context_reaches_the_acp_wire_prompt() {
    let h = setup(false, None).await;
    let prompt = prompt_seen_by_the_agent(&h).await;

    assert!(
        !prompt.contains(NOTE_TITLE),
        "with precognition disabled no kiln context may reach the agent; \
         the agent received: {prompt:?}"
    );
    assert!(
        prompt.contains(USER_MESSAGE),
        "the user's own message must still reach the ACP prompt; \
         the agent received: {prompt:?}"
    );
}

/// A plugin's `transform_context` block reaches the external agent too.
///
/// This is the capability-grade half of the seam: a plugin can put words in
/// front of an agent Crucible does not run. It is folded into the same
/// message array Precognition uses, and the ACP handle linearizes both, so
/// a change that forwarded only the built-in producer would leave every
/// in-process test green.
#[tokio::test]
async fn a_plugin_transform_context_block_reaches_the_acp_wire_prompt() {
    const INJECTED: &str = "[plugin] the tolerance register is read-only on Tuesdays";
    let plugin = format!(
        r#"
cru.on("transform_context", function(ctx, event)
  local msgs = event.messages
  table.insert(msgs, {{ role = "system", content = "{INJECTED}" }})
  return {{ messages = msgs }}
end)
return {{ name = "injector", version = "0.1.0", description = "context injector" }}
"#
    );

    let h = setup(false, Some(&plugin)).await;
    let prompt = prompt_seen_by_the_agent(&h).await;

    assert!(
        prompt.contains(INJECTED),
        "a plugin's transform_context block must reach the ACP prompt; \
         the agent received: {prompt:?}"
    );
}
