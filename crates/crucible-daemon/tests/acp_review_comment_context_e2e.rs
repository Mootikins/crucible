//! An attached review comment on the ACP path, asserted on the ACP wire.
//!
//! A chat message can attach a stored review comment. The daemon resolves the
//! reference and builds a `<system-message kind="review-comment">` block. The two
//! agent kinds then take different routes, and that difference is the whole
//! point of this file:
//!
//! - An internal agent gets the block as ACCEPTED CONTEXT. The daemon appends
//!   a `context_injected` event to `session.jsonl`, so replay, undo and fork
//!   keep the block with its System role.
//! - An ACP agent owns its history. The daemon must NOT write that line; the
//!   block rides the one turn through the attachment seam in `send.rs`, and
//!   `acp_prompt_text` linearizes it into the prompt the agent process reads.
//!
//! `agent_manager::tests::review_comment_context` covers the internal route
//! against an in-process capturing agent. Nothing covered the ACP route at
//! all, and an in-process mock cannot cover it: the ACP handle is the one
//! consumer that re-linearizes the message array instead of passing it
//! through, so a block that never reaches the attachment seam is invisible to
//! every in-process assertion.
//!
//! So these tests drive a real `mock-acp-agent` PROCESS and read the frame log
//! it wrote (`log` in the `MockScript`). The assertions are on bytes that crossed
//! the process boundary. The whole chain runs through the daemon's own socket:
//! `diff.comment` stores the comment, `session.send_message` carries the
//! reference, and the refusals come back as JSON-RPC errors.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crucible_core::config::{AcpConfig, BackendType};
use crucible_core::diff::{CommentRef, DiffsetSource};
use crucible_core::session::{CommentSide, PhysicalRoot, SessionAgent};
use crucible_daemon::rpc_client::{DiffCommentRequest, SessionCreateParams};
use crucible_daemon::test_support::{git, init_repo, kiln_name};
use crucible_daemon::{BindWithPluginConfigParams, DaemonClient, Server, SessionEvent};
use tempfile::TempDir;
use tokio::sync::mpsc::UnboundedReceiver;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, MockScript, Step};
use mock_agent_bin::{mock_profile, mock_session_agent, profile_session_agent, MOCK_PROFILE};

/// What the mock agent streams, so the turn ends with a real answer.
const ANSWER: &str = "understood";

/// The body of the stored comment. Distinctive, so its presence in the
/// captured prompt cannot be an accident of the user's own words.
const COMMENT_BODY: &str = "why does this line shout?";

/// What the user types beside the reference.
const USER_MESSAGE: &str = "please look at my note";

/// The kiln every session attaches.
const KILN: &str = "notes";

/// A cold spawn, an ACP handshake and a turn.
const TURN_TIMEOUT: Duration = Duration::from_secs(90);

/// A daemon over a temp data root, with one kiln and one git project.
struct Fixture {
    _dir: TempDir,
    repo: PathBuf,
    data: PathBuf,
    root: PhysicalRoot,
    client: DaemonClient,
    events: UnboundedReceiver<SessionEvent>,
    shutdown: tokio::sync::broadcast::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Fixture {
    /// A repository on `feature` with one uncommitted edit, registered as a
    /// project, and a daemon that serves it.
    async fn start() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = TempDir::new().expect("temp dir");
        let repo = dir.path().join("repo");
        init_repo(&repo, &[("a.rs", "one\ntwo\nthree\n")]).await;
        git(&repo, &["branch", "-M", "main"]).await;
        git(&repo, &["checkout", "-q", "-b", "feature"]).await;
        std::fs::write(repo.join("a.rs"), "one\nTWO\nthree\n").expect("edit the file");

        let kiln = dir.path().join("kiln");
        std::fs::create_dir_all(&kiln).expect("kiln dir");
        let data = dir.path().join("data");
        let socket = dir.path().join("daemon.sock");
        // The daemon runs an ACP agent only through a profile, so the mock
        // binary gets one. The script travels in the session agent's env.
        let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
            path: socket.clone(),
            config_home: Some(data.join("config")),
            data_home: Some(data.clone()),
            app_config: Some(serde_json::json!({
                "kilns": { KILN: kiln.to_string_lossy() }
            })),
            acp_config: Some(AcpConfig {
                agents: [(MOCK_PROFILE.to_string(), mock_profile(BTreeMap::new()))].into(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .expect("bind the daemon");
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());

        let (client, events) = DaemonClient::connect_to_with_events(&socket)
            .await
            .expect("connect to the daemon socket");
        client
            .session_subscribe(&["*"])
            .await
            .expect("subscribe to every session");
        let project = client
            .project_register(&repo)
            .await
            .expect("register the repository");

        Self {
            _dir: dir,
            repo,
            data,
            root: PhysicalRoot::from_top_level(project.path),
            client,
            events,
            shutdown,
            task,
        }
    }

    /// The branch diffset of the repository: the merge base with the default
    /// branch, against the working tree.
    fn source(&self) -> DiffsetSource {
        DiffsetSource::Branch {
            root: self.root.clone(),
            base: String::new(),
            head: None,
        }
    }

    /// Store one comment on lines 1 and 2 of the current side of `a.rs`, and
    /// return a reference to it.
    ///
    /// The range spans a kept line and the edited one, so the hunk holds both
    /// the removed row and the added row. A range over the edited line alone
    /// would show the addition only, and the test could not tell a real hunk
    /// from a single quoted line.
    async fn comment(&self) -> CommentRef {
        let reply = self
            .client
            .diff_comment(DiffCommentRequest {
                source: self.source(),
                root: Some(self.root.clone()),
                path: "a.rs".to_string(),
                from: None,
                side: CommentSide::Current,
                line_start: 1,
                line_end: Some(3),
                body: COMMENT_BODY.to_string(),
                author: None,
            })
            .await
            .expect("store the comment");
        CommentRef {
            id: reply.comment.id,
            source: self.source(),
        }
    }

    /// Create a chat session over the repository. `agent_type` is what
    /// `session.create` records; `agent` is what it then runs.
    /// Make `endpoint` the configured `chat.endpoint`, through the RPC a
    /// settings UI uses. The daemon refuses a loopback endpoint that a request
    /// names unless the operator configured it, and a mock provider listens
    /// on loopback.
    async fn configure_endpoint(&self, endpoint: &str) {
        self.client
            .call(
                "config.set",
                serde_json::json!({ "values": { "chat.endpoint": endpoint } }),
            )
            .await
            .expect("set chat.endpoint");
    }

    async fn session(&self, agent_type: &str, agent: &SessionAgent) -> String {
        let created = self
            .client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kilns: vec![kiln_name(KILN)],
                workspace: Some(self.repo.clone()),
                recording_mode: None,
                recording_path: None,
                agent_type: Some(agent_type.to_string()),
                isolation: None,
            })
            .await
            .expect("create the session");
        let id = created["session_id"]
            .as_str()
            .expect("session.create answers a session_id")
            .to_string();
        self.client
            .session_configure_agent(&id, agent)
            .await
            .expect("configure the agent");
        id
    }

    /// Wait until `session` reports `event`.
    async fn wait_for(&mut self, session: &str, event: &str) {
        let deadline = tokio::time::Instant::now() + TURN_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!left.is_zero(), "{event} never arrived for {session}");
            match tokio::time::timeout(left, self.events.recv()).await {
                Ok(Some(got)) if got.session_id == session && got.event == event => return,
                Ok(Some(_)) => continue,
                Ok(None) => panic!("the event stream closed before {event}"),
                Err(_) => panic!("{event} never arrived for {session}"),
            }
        }
    }

    /// Every line of the session log, as text.
    fn log(&self, session: &str) -> String {
        let path = self
            .data
            .join("sessions")
            .join(session)
            .join("session.jsonl");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// The accepted-context lines of the session log.
    ///
    /// `SessionInput::accept` writes one `context_injected` event per
    /// accepted block. An ACP session must have none.
    fn injections(&self, session: &str) -> Vec<String> {
        self.log(session)
            .lines()
            .filter(|line| line.contains("\"event\":\"context_injected\""))
            .map(str::to_string)
            .collect()
    }

    async fn stop(self) {
        drop(self.client);
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// An ACP agent that runs the mock binary, streams `ANSWER` and logs each
/// frame it receives to `capture`.
fn acp_agent(capture: &Path) -> SessionAgent {
    let mut agent = profile_session_agent(MOCK_PROFILE);
    let script = MockScript {
        turn: vec![Step::Text(ANSWER.to_string())],
        log: Some(capture.to_path_buf()),
        ..MockScript::default()
    };
    agent.env_overrides.extend([script.env()]);
    agent
}

/// The text of the last prompt that the agent process logged, or `None`
/// when it logged no prompt.
fn last_prompt(capture: &Path) -> Option<String> {
    logged(capture, "session/prompt").last().map(|prompt| {
        prompt["prompt"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| block["text"].as_str())
            .collect()
    })
}

/// An internal agent whose provider endpoint the test owns.
fn internal_agent(endpoint: &str) -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are helpful.".to_string(),
        endpoint: Some(endpoint.to_string()),
        ..mock_session_agent("unused")
    }
}

/// A provider that answers every turn with an empty stream.
///
/// The internal turn has to RUN, because the daemon writes the accepted
/// context while assembling it. What the model says does not matter here.
async fn empty_provider() -> wiremock::MockServer {
    use wiremock::matchers::method;
    use wiremock::{Mock, ResponseTemplate};

    let server = wiremock::MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: [DONE]\n\n"),
        )
        .mount(&server)
        .await;
    server
}

/// The headline: the block the daemon built for an attached comment reaches
/// the ACP agent's prompt, with the file, the range and the hunk.
///
/// Without this the external agent reads "please look at my note" and nothing
/// else, and the comment the user attached might as well not exist.
#[tokio::test]
async fn an_attached_comment_reaches_the_acp_wire_prompt() {
    let mut fixture = Fixture::start().await;
    let capture = fixture._dir.path().join("acp_prompt.txt");
    let reference = fixture.comment().await;
    let session = fixture.session("acp", &acp_agent(&capture)).await;

    fixture
        .client
        .session_send_message_with_comments(
            &session,
            USER_MESSAGE,
            std::slice::from_ref(&reference),
            true,
        )
        .await
        .expect("the daemon accepts the message");
    fixture.wait_for(&session, "message_complete").await;

    let prompt = last_prompt(&capture).expect("the agent process must log the prompt it received");

    for part in [
        "<system-message kind=\"review-comment\" source=\"human\">\n".to_string(),
        "The user attached comments on changed files:\n".to_string(),
        format!("- a.rs:1-2: \"{COMMENT_BODY}\"\n"),
        "    section: Branch changes: the working tree against main\n".to_string(),
        "    -two\n".to_string(),
        "    +TWO\n".to_string(),
        "</system-message>".to_string(),
        USER_MESSAGE.to_string(),
    ] {
        assert!(
            prompt.contains(&part),
            "{part:?} must reach the ACP prompt; the agent received: {prompt:?}"
        );
    }
    // One injection is one element.
    assert_eq!(prompt.matches("<system-message").count(), 1, "{prompt:?}");
    // The block frames the message, so it precedes it.
    assert!(
        prompt.find("<system-message").unwrap() < prompt.find(USER_MESSAGE).unwrap(),
        "the block must come before the user content; the agent received: {prompt:?}"
    );

    fixture.stop().await;
}

/// The control. A second turn with no reference must carry no block, or the
/// assertion above would also pass against a daemon that pastes the comment
/// into every prompt of the session.
#[tokio::test]
async fn a_later_turn_without_a_reference_carries_no_block() {
    let mut fixture = Fixture::start().await;
    let capture = fixture._dir.path().join("acp_prompt.txt");
    let reference = fixture.comment().await;
    let session = fixture.session("acp", &acp_agent(&capture)).await;

    fixture
        .client
        .session_send_message_with_comments(
            &session,
            USER_MESSAGE,
            std::slice::from_ref(&reference),
            true,
        )
        .await
        .expect("the daemon accepts the first message");
    fixture.wait_for(&session, "message_complete").await;

    fixture
        .client
        .session_send_message(&session, "and now something else", true)
        .await
        .expect("the daemon accepts the second message");
    fixture.wait_for(&session, "message_complete").await;

    let prompt = last_prompt(&capture).expect("the second prompt");
    assert!(
        !prompt.contains("review-comment"),
        "a turn that attaches nothing must carry no block; the agent received: {prompt:?}"
    );
    assert!(prompt.contains("and now something else"));

    fixture.stop().await;
}

/// The difference between the two routes, asserted in one place.
///
/// The ACP session's stored history must NOT gain the accepted-context line,
/// because the agent owns its history and would see the block twice. The
/// internal session's history must gain it, because the daemon owns that
/// history and replay has no other source for the block. One test holds both
/// halves, so the two routes cannot silently converge on either answer.
#[tokio::test]
async fn only_the_internal_route_writes_the_block_into_the_stored_history() {
    let provider = empty_provider().await;
    let mut fixture = Fixture::start().await;
    let capture = fixture._dir.path().join("acp_prompt.txt");
    let reference = fixture.comment().await;

    let acp = fixture.session("acp", &acp_agent(&capture)).await;
    fixture.configure_endpoint(&provider.uri()).await;
    let internal = fixture
        .session("internal", &internal_agent(&provider.uri()))
        .await;

    fixture
        .client
        .session_send_message_with_comments(
            &acp,
            USER_MESSAGE,
            std::slice::from_ref(&reference),
            true,
        )
        .await
        .expect("the daemon accepts the ACP message");
    // The ACP turn has to finish: the agent process logs the prompt.
    fixture.wait_for(&acp, "message_complete").await;

    fixture
        .client
        .session_send_message_with_comments(
            &internal,
            USER_MESSAGE,
            std::slice::from_ref(&reference),
            true,
        )
        .await
        .expect("the daemon accepts the internal message");
    // The internal turn does not have to finish. The daemon writes the
    // accepted context while it assembles the input, which is before it
    // calls the provider, so the reply to the send is already later than
    // the write.
    fixture.wait_for(&internal, "user_message").await;

    assert!(
        fixture.injections(&acp).is_empty(),
        "an ACP session owns its history and must store no injected context; log: {}",
        fixture.log(&acp)
    );

    let accepted = fixture.injections(&internal);
    assert_eq!(
        accepted.len(),
        1,
        "the internal session must store one injected block; log: {}",
        fixture.log(&internal)
    );
    let line: serde_json::Value = serde_json::from_str(&accepted[0]).expect("the line is JSON");
    assert_eq!(
        line["data"]["role"], "system",
        "the accepted block keeps its System role: {line}"
    );
    let stored = line["data"]["content"]
        .as_str()
        .expect("the stored block is text");
    assert!(
        stored.contains(COMMENT_BODY),
        "the stored block is the review comment: {stored:?}"
    );
    // The log keeps the kind; the turn adds the one element, live and on
    // replay.
    assert_eq!(
        (&line["data"]["kind"], &line["data"]["source"]),
        (
            &serde_json::json!("review-comment"),
            &serde_json::json!("human")
        ),
        "{line}"
    );

    // Both agents saw the same block; only their histories differ.
    let prompt = last_prompt(&capture).expect("the ACP prompt");
    assert!(
        prompt.contains(COMMENT_BODY),
        "the ACP agent still receives the block in its prompt: {prompt:?}"
    );

    fixture.stop().await;
}

/// An unknown comment id refuses the message, with one wording for both
/// routes. A refusal that differed by agent kind would mean the resolution
/// had been duplicated per route.
#[tokio::test]
async fn an_unknown_comment_id_refuses_both_routes_alike() {
    let fixture = Fixture::start().await;
    let capture = fixture._dir.path().join("acp_prompt.txt");

    let acp = fixture.session("acp", &acp_agent(&capture)).await;
    // The refusal happens before the daemon builds the agent, so this
    // endpoint is never called.
    fixture.configure_endpoint("http://127.0.0.1:1/").await;
    let internal = fixture
        .session("internal", &internal_agent("http://127.0.0.1:1/"))
        .await;
    let unknown = CommentRef {
        id: "nope".to_string(),
        source: fixture.source(),
    };

    let mut refusals = Vec::new();
    for session in [&acp, &internal] {
        let error = fixture
            .client
            .session_send_message_with_comments(
                session,
                USER_MESSAGE,
                std::slice::from_ref(&unknown),
                true,
            )
            .await
            .expect_err("the daemon refuses an unknown comment")
            .to_string();
        assert!(
            error.contains("has no comment nope"),
            "the refusal must name the unknown comment; got {error:?}"
        );
        refusals.push(error);
    }
    assert_eq!(
        refusals[0], refusals[1],
        "both routes must refuse in the same words"
    );

    // The refusal stops the turn: the agent process saw no prompt at all.
    assert!(
        last_prompt(&capture).is_none(),
        "a refused message must start no ACP turn"
    );

    fixture.stop().await;
}
