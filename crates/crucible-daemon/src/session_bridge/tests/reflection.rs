//! The shipped plugin, not a replacement hook: end → provider → proposal →
//! rejection → the rejected title for the next pass.

use super::*;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::observe::LogEvent;
use crate::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_core::session::SessionState;
use crucible_lua::{manifest::PluginSource, DaemonSessionApi};
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The reviewer's note in every provider answer of the fixture.
const NOTE: &str = "# Remember\n\nUse the daemon as the only storage owner.\n";

/// The reviewer's chat route, and only it. The daemon also POSTs `/api/show`
/// to probe the model's context window when it creates the pass's session
/// (the session-create setup task), and whether that probe lands before the
/// test tears the mock down is a race: an endpoint that answered it made the
/// "one tool call and one continuation" count below three on a fresh runner.
/// A chat request is the one that reaches the chat route.
fn is_chat_request(request: &wiremock::Request) -> bool {
    request.method.as_str() == "POST" && request.url.path().ends_with("/api/chat")
}

/// A daemon with the shipped reflection plugin active, a provider fixture that
/// proposes [`NOTE`], and the kiln `knowledge`.
struct Rig {
    // Dropped last: the provider environment stays clear while the rig lives.
    _env_guards: Vec<crucible_core::test_support::EnvVarGuard>,
    _env_lock: std::sync::MutexGuard<'static, ()>,
    tmp: TempDir,
    kiln: TempDir,
    provider: MockServer,
    sessions: Arc<crate::session_manager::SessionManager>,
    agents: Arc<AgentManager>,
    ctx: Arc<RpcContext>,
    bridge: Arc<DaemonSessionBridge>,
    lua: Arc<mlua::Lua>,
    observed: broadcast::Receiver<crucible_core::protocol::SessionEventMessage>,
}

impl Rig {
    /// Boot the rig. `hook_lua` runs on the plugin VM before the reflection
    /// plugin activates, so a test can put a stand-in plugin hook there.
    ///
    /// The environment lock is held across the awaits on purpose, for the
    /// life of the rig: nextest runs each test in its own process, so it
    /// waits for nothing, and it keeps the provider environment clear.
    #[allow(clippy::await_holding_lock)]
    async fn new(hook_lua: Option<&str>) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        // The provider table is the one this test injects: the daemon's own
        // enumeration also reads the process environment's credentials, so a
        // developer's `GLM_AUTH_TOKEN` (or `OPENAI_API_KEY`, …) would put a
        // real endpoint on the setup path. nextest runs each test in its own
        // process, so the guard races with nothing.
        let env_lock = crate::agent_manager::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let env_guards = crate::agent_manager::clear_provider_env();

        let provider = MockServer::start().await;
        Mock::given(is_chat_request)
            .respond_with(move |request: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let has_result = body["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool");
                let message = if has_result {
                    json!({"role": "assistant", "content": "Saved one lesson for review."})
                } else {
                    json!({"role": "assistant", "content": "", "tool_calls": [{"id": "lesson", "function": {
                        "name": "create_note", "arguments": {"path": "Remember.md", "content": NOTE}
                    }}]})
                };
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/x-ndjson")
                    .set_body_string(format!("{}\n{}\n", json!({"model": "reflection-fixture", "message": message, "done": false}), json!({"done": true, "done_reason": "stop"})))
            })
            .mount(&provider)
            .await;

        let tmp = TempDir::new().unwrap();
        let kiln = TempDir::new().unwrap();
        let sessions = temp_session_manager_with_kilns(&[("knowledge", kiln.path())]);
        let shared_loader = Arc::new(tokio::sync::Mutex::new(None));
        let (events, observed) = broadcast::channel(128);
        let kilns = Arc::new(KilnManager::with_event_tx(
            events.clone(),
            Some(crucible_core::config::EmbeddingProviderConfig::mock(Some(
                384,
            ))),
            crucible_core::config::default_max_precognition_chars(),
        ));
        let mut llm = bridge_llm_config();
        llm.providers.get_mut("ollama").unwrap().endpoint = Some(provider.uri());
        let mut loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
        let lua = loader.plugin_lua();
        let previous = crucible_lua::set_source(&lua, crucible_lua::LuaSource::Builtin);
        lua.load(crucible_lua::BUILTIN_INIT_LUA).exec().unwrap();
        crucible_lua::set_source(&lua, previous);
        let agents = Arc::new(
            AgentManager::new(AgentManagerParams {
                kiln_manager: kilns.clone(),
                session_manager: sessions.clone(),
                background_manager: Arc::new(BackgroundJobManager::new(events.clone())),
                mcp_gateway: None,
                llm_config: Some(llm),
                acp_config: None,
                context_config: None,
                permission_config: None,
                plugin_loader: Some(shared_loader.clone()),
                card_roots: Default::default(),
                review_snapshot_root: tmp.path().join("snapshots"),
            })
            .with_modes(Some(loader.mode_registry())),
        );
        let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
            kilns,
            sessions.clone(),
            agents.clone(),
            Arc::new(crate::project_manager::ProjectManager::new(
                tmp.path().join("projects.json"),
            )),
            events,
            tmp.path().into(),
            shared_loader.clone(),
        ));
        let bridge = Arc::new(DaemonSessionBridge::new(ctx.clone()));
        loader.upgrade_with_sessions(bridge.clone()).unwrap();
        loader
            .upgrade_with_tools(Arc::new(
                crate::tools_bridge::DaemonToolsBridge::new(
                    Arc::new(crate::tools::workspace::WorkspaceTools::new(tmp.path())),
                    None,
                )
                .with_active_tools(agents.active_tools(), sessions.clone()),
            ))
            .unwrap();
        agents.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
        agents.set_daemon_permissions(loader.permission_registry());
        agents.set_plugin_tool_registry(loader.plugin_registry());
        agents.set_isolation(loader.isolation());
        if let Some(code) = hook_lua {
            loader.eval(code).await.unwrap();
        }
        loader
            .add_plugin_paths(&[(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/plugins"),
                PluginSource::Runtime,
            )])
            .unwrap();
        loader.activate_plugin("reflection").await.unwrap();
        loader.eval(r#"require("reflection").setup({ model = "reflection-fixture", min_turns = 1, timeout = 5 })"#).await.unwrap();
        *shared_loader.lock().await = Some(loader);

        Self {
            _env_guards: env_guards,
            _env_lock: env_lock,
            tmp,
            kiln,
            provider,
            sessions,
            agents,
            ctx,
            bridge,
            lua,
            observed,
        }
    }

    /// A finished chat session in `knowledge` with one user turn, on
    /// `workspace`.
    async fn finished_session(
        &self,
        workspace: Option<PathBuf>,
    ) -> crucible_core::session::Session {
        let source = self
            .sessions
            .create_session(
                SessionType::Chat,
                vec![kiln_name("knowledge")],
                workspace,
                None,
            )
            .await
            .unwrap();
        self.transcript(&source).await;
        source
    }

    /// Write the transcript that the reviewer reads into `session`.
    async fn transcript(&self, session: &crucible_core::session::Session) {
        for event in [
            LogEvent::user("What did we learn?"),
            LogEvent::assistant("Keep storage in the daemon."),
        ] {
            self.sessions
                .storage()
                .append_event(session, &event.to_jsonl().unwrap())
                .await
                .unwrap();
        }
    }

    /// Run the pass for `session_id` as the reflection plugin runs it: in a
    /// task that holds no plugin runtime, under the plugin's own source.
    /// Answers with the reviewer's summary, or `None` when the pass skipped
    /// the session or failed.
    async fn review(&self, session_id: &str) -> Option<String> {
        let previous = crucible_lua::set_source(
            &self.lua,
            crucible_lua::LuaSource::Plugin("reflection".into()),
        );
        let output = self
            .lua
            .load(format!(
                r#"return require("reflection").run({{ id = "{session_id}" }})"#
            ))
            .eval_async::<Option<String>>()
            .await;
        crucible_lua::set_source(&self.lua, previous);
        output.expect("the pass raised")
    }

    /// The pass sessions: every session of type `plugin`.
    fn passes(&self) -> Vec<crucible_core::session::Session> {
        self.sessions
            .list_sessions()
            .into_iter()
            .filter(|s| s.session_type == SessionType::Plugin)
            .map(|s| self.sessions.get_session(&s.id).unwrap())
            .collect()
    }
}

#[tokio::test]
async fn shipped_reflection_proposes_a_note_on_session_end() {
    let rig = Rig::new(None).await;
    let Rig {
        tmp,
        kiln,
        provider,
        sessions,
        agents,
        ctx,
        bridge,
        observed,
        ..
    } = &rig;
    let mut observed = observed.resubscribe();
    let note = NOTE;
    // Its own directory, not the rig's data home: the pass now runs on this
    // workspace, and the data home holds the proposal store and snapshots
    // that the pass's review ledger would read as edits.
    let workspace = tmp.path().join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    let source = rig.finished_session(Some(workspace)).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        ctx.session_lifecycle.fire_session_end(&source.id),
    )
    .await
    .expect("reflection end hook completes without re-entering the loader lock");
    // The hook starts the pass in a task, because the pass's own session runs
    // the start hooks, and the hook holds the plugin runtime. Wait for the
    // whole end of the pass: the Lua end marks the session ended, then drops
    // its tool set and its provider handle.
    tokio::time::timeout(Duration::from_secs(30), async {
        while !sessions.list_sessions().iter().any(|s| {
            s.session_type == SessionType::Plugin
                && s.state == SessionState::Ended
                && agents.active_tools().get(&s.id).is_none()
                && !agents.has_cached_agent(&s.id)
        }) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect(
        "the reflection pass did not end its session, drop its tool set and release its \
         provider handle",
    );

    let passes: Vec<_> = sessions
        .list_sessions()
        .into_iter()
        .filter(|s| s.session_type == SessionType::Plugin)
        .collect();
    assert_eq!(
        passes.len(),
        1,
        "one source end produces one reflection pass"
    );
    let pass = sessions.get_session(&passes[0].id).unwrap();
    assert_eq!(
        pass.state,
        SessionState::Ended,
        "the aux session is ended on completion"
    );
    assert_eq!(
        pass.workspace, source.workspace,
        "the reviewer runs on the workspace of the session it reviews"
    );
    assert_eq!(pass.kilns, source.kilns);
    let config = pass.agent.unwrap();
    assert_eq!(config.model, "reflection-fixture");
    assert_eq!(config.mode.as_deref(), Some("propose"));
    assert!(config.mcp_servers.is_empty());
    let requests: Vec<serde_json::Value> = provider
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(is_chat_request)
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    let mut seen = Vec::new();
    while let Ok(event) = observed.try_recv() {
        seen.push(event);
    }
    // The pass proposes: the note is in one proposal, and the disk does not
    // change.
    assert!(
        !kiln.path().join("Remember.md").exists(),
        "a propose-mode pass changes no note on disk"
    );
    let proposals = agents.proposals().list(false).unwrap();
    assert_eq!(
        proposals.len(),
        1,
        "the reviewer proposed one note; requests: {requests:?}; events: {seen:?}"
    );
    let proposal = &proposals[0];
    assert_eq!(
        proposal.author,
        crucible_core::proposal::ProposalAuthor::Plugin {
            name: "reflection".into()
        }
    );
    assert_eq!(proposal.writes.len(), 1);
    assert_eq!(proposal.writes[0].path, "Remember.md");
    assert_eq!(proposal.writes[0].new_text, note);
    assert_eq!(
        agents.active_tools().get(&pass.id),
        None,
        "ending drops the transient tool set"
    );
    assert!(
        !agents.has_cached_agent(&pass.id),
        "ending releases the provider handle"
    );

    assert_eq!(requests.len(), 2, "one tool call and one continuation");
    let request = &requests[0];
    assert_eq!(request["model"], "reflection-fixture");
    assert!(request["messages"]
        .to_string()
        .contains("You are a reflection reviewer"));
    assert!(request["messages"]
        .to_string()
        .contains("Keep storage in the daemon."));
    let tools: Vec<_> = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert!(tools.contains(&"create_note"), "{tools:?}");
    assert!(
        !tools
            .iter()
            .any(|t| ["bash", "read_file", "write_file"].contains(t)),
        "{tools:?}"
    );

    // A proposal writes no hunk in the pass's review ledger.
    assert!(
        bridge
            .review_list_hunks(pass.id.to_string())
            .await
            .unwrap()
            .is_empty(),
        "the pass's note is a proposal, not a hunk"
    );
    // The next pass reads the rejected title through `cru.proposals`.
    agents
        .proposals()
        .reject(&proposal.id, Some("a duplicate".into()))
        .unwrap();
    let rejected = bridge.rejected_proposals(20).await.unwrap();
    assert_eq!(rejected.len(), 1, "{rejected:?}");
    assert_eq!(rejected[0]["id"], proposal.id.to_string());
    assert_eq!(rejected[0]["title"], "Change Remember.md");
    assert_eq!(rejected[0]["reason"], "a duplicate");
    assert_eq!(rejected[0]["paths"], json!(["Remember.md"]));
    assert!(
        bridge.rejected_proposals(0).await.unwrap().is_empty(),
        "the limit caps the rows"
    );
    assert!(
        bridge
            .review_list_hunks(source.id.to_string())
            .await
            .unwrap()
            .is_empty(),
        "the source owns no edits from the reflection pass"
    );
    ctx.session_lifecycle.fire_session_end(&source.id).await;
    assert_eq!(
        sessions.list_sessions().len(),
        2,
        "repeated teardown does not spawn another pass"
    );
}

/// A stand-in for the `oci` plugin with a configured image: it isolates every
/// session that does not opt out, and it refuses a session with no workspace,
/// as `oci` does ("oci: session has no workspace to isolate").
const ISOLATES_EVERY_WORKSPACE: &str = r#"
cru.on_session_start(function(session)
  if session.isolation == false then return end
  if not session.workspace or session.workspace == "" then
    error("sandbox: session has no workspace to isolate")
  end
  _G.isolated_workspaces = (_G.isolated_workspaces or "") .. session.workspace .. ";"
  cru.isolation.require{ session = session.id, plugin = "sandbox" }
end, { required = true })
"#;

/// The pass runs on the workspace of the session it reviews, so an isolating
/// plugin isolates the pass as it isolated that session.
///
/// Before the fix the pass had no workspace. The isolating plugin refused it,
/// and the pass only wrote a log line.
#[tokio::test]
async fn an_isolated_session_gets_its_review_on_its_own_workspace() {
    let rig = Rig::new(Some(ISOLATES_EVERY_WORKSPACE)).await;
    let workspace = rig.tmp.path().join("project");
    std::fs::create_dir_all(&workspace).unwrap();
    let source = rig.finished_session(Some(workspace.clone())).await;

    let output = rig.review(&source.id).await;

    assert!(
        output.is_some(),
        "the isolating plugin refused the pass of session {}, so the session got no review",
        source.id
    );
    let passes = rig.passes();
    assert_eq!(passes.len(), 1, "one review makes one pass");
    assert_eq!(passes[0].workspace, Some(workspace.clone()));
    let isolated: Option<String> = rig.lua.globals().get("isolated_workspaces").unwrap();
    assert_eq!(
        isolated,
        Some(format!("{};", workspace.display())),
        "the isolating plugin isolated the pass on the reviewed workspace"
    );
    assert_eq!(
        rig.agents.proposals().list(false).unwrap().len(),
        1,
        "the isolated pass proposed its note"
    );
}

/// A session with no workspace still gets its review, and the pass has no
/// workspace either.
#[tokio::test]
async fn a_session_with_no_workspace_gets_a_review_with_no_workspace() {
    let rig = Rig::new(None).await;
    let source = rig.finished_session(None).await;

    let output = rig.review(&source.id).await;

    assert!(
        output.is_some(),
        "the session with no workspace got no review"
    );
    let passes = rig.passes();
    assert_eq!(passes.len(), 1, "one review makes one pass");
    assert_eq!(passes[0].workspace, None);
}

/// Create a finished session as the plugin `courier` creates one, through
/// `cru.session.create` under the plugin's own source. When `request_review`
/// is true, the plugin then asks for a review with the `reflection:request`
/// event on `cru.emitter.global()`.
async fn courier_session(rig: &Rig, request_review: bool) -> String {
    let previous =
        crucible_lua::set_source(&rig.lua, crucible_lua::LuaSource::Plugin("courier".into()));
    let created = rig
        .lua
        .load(format!(
            r#"
            local s, err = cru.session.create({{ type = "chat", kilns = {{ "knowledge" }} }})
            assert(s, err)
            if {request_review} then
                cru.emitter.global():emit("reflection:request", s.id)
            end
            return s.id
            "#
        ))
        .eval_async::<String>()
        .await;
    crucible_lua::set_source(&rig.lua, previous);
    let id = created.expect("the plugin creates its session");
    let session = rig.sessions.get_session(&id).unwrap();
    rig.transcript(&session).await;
    id
}

/// A session that a plugin created is reviewed only when that plugin asks for
/// a review. Before the fix the daemon kept no record that a plugin created a
/// `chat` session, so every such session was reviewed like a user's own.
#[tokio::test]
async fn a_plugin_session_is_reviewed_only_when_its_plugin_asks() {
    let rig = Rig::new(None).await;

    let silent = courier_session(&rig, false).await;
    assert_eq!(
        rig.review(&silent).await,
        None,
        "plugin session {silent} ended with no reflection:request event and got a review"
    );
    assert!(
        rig.passes().is_empty(),
        "no pass for a session nobody asked to review"
    );

    let asked = courier_session(&rig, true).await;
    assert!(
        rig.review(&asked).await.is_some(),
        "plugin session {asked} asked for a review with reflection:request and got none"
    );
    assert_eq!(rig.passes().len(), 1, "one request makes one pass");
}

/// A session that a user created is reviewed as before, with no event.
#[tokio::test]
async fn a_user_session_is_reviewed_without_a_request() {
    let rig = Rig::new(None).await;
    let source = rig.finished_session(None).await;

    assert!(rig.review(&source.id).await.is_some());
}
