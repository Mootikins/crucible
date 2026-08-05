//! Integration test: drive a real `CrucibleAcpClient` against recorded
//! fixtures and assert the streaming round trip produces the expected data.
//!
//! One case per agent we have a live capture for. Each case replays the whole
//! `initialize` → `session/new` → `session/prompt` handshake off the recorded
//! wire and asserts the *values* the daemon lifted out of it: the agent's
//! self-reported identity, the session id, the ordered chunk shapes the
//! streaming callback saw, the reassembled answer text, the stop reason and
//! the token usage. Nothing here is "it didn't crash" — every field asserted
//! is one the TUI or the session store reads.
//!
//! ## Recording
//!
//! Fixtures were captured live against real agent binaries via
//! `just record-acp-fixture <agent>`, which sets `CRUCIBLE_ACP_RECORD_DIR`
//! and drives one prompt through the daemon. Recorded files are then
//! sanitized by replacing `/home/<user>` → `<HOME>` and copied to
//! `tests/fixtures/acp/recorded/<agent>/basic-chat.jsonl`. Replay itself is
//! hermetic: it never spawns an agent, so these tests are **not** `#[ignore]`d
//! and run on any machine.
//!
//! ## Why not the `acp_support::parity` harness
//!
//! `ShapeProjector`/`shapes()` project a `TurnEvent` stream, which is what
//! `AcpAgentHandle` emits. `AcpAgentHandle::new` spawns a real agent process
//! and has no transport-injection constructor, so a fixture cannot be driven
//! through it — this test sits one layer lower, on `CrucibleAcpClient`, whose
//! per-turn output is a `StreamingChunk` callback. [`ChunkShape`] below is the
//! same idea (ordered, value-normalized projection with adjacent runs
//! coalesced) applied at the layer this test can actually reach.

use std::path::{Path, PathBuf};

use agent_client_protocol::{
    ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, StopReason, TextContent,
};
use crucible_daemon::acp::client::replay::{ReplayFixture, ReplayOutcome};
use crucible_daemon::acp::{CrucibleAcpClient, StreamingChunk};

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

/// The rendering-relevant discriminant of a [`StreamingChunk`].
///
/// Chunk *boundaries* are an artifact of the agent's flush cadence — Claude
/// splits "Hello to you!" across three notifications, one of them empty —
/// so a raw chunk count is not a contract. The ordered sequence of kinds is:
/// it decides whether a turn renders as thinking-then-answer, answer-only, or
/// a tool block. Adjacent runs are coalesced by [`coalesce`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkShape {
    Text,
    Thinking,
    ToolStart,
    ToolEnd,
    ToolDiffUpdate,
    ContextWindow,
}

fn shape_of(chunk: &StreamingChunk) -> ChunkShape {
    match chunk {
        StreamingChunk::Text(_) => ChunkShape::Text,
        StreamingChunk::Thinking(_) => ChunkShape::Thinking,
        StreamingChunk::ToolStart { .. } => ChunkShape::ToolStart,
        StreamingChunk::ToolEnd { .. } => ChunkShape::ToolEnd,
        StreamingChunk::ToolDiffUpdate { .. } => ChunkShape::ToolDiffUpdate,
        StreamingChunk::ContextWindow { .. } => ChunkShape::ContextWindow,
    }
}

fn coalesce(shapes: &[ChunkShape]) -> Vec<ChunkShape> {
    let mut out: Vec<ChunkShape> = Vec::new();
    for shape in shapes {
        if out.last() != Some(shape) {
            out.push(*shape);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Case table
// ---------------------------------------------------------------------------

struct ExpectedUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    cache_read_tokens: Option<u32>,
    cache_creation_tokens: Option<u32>,
}

enum TurnExpectation {
    /// The turn completed and produced a `PromptResponse`.
    Completed {
        /// Coalesced chunk shapes, in order.
        shapes: &'static [ChunkShape],
        /// The reassembled answer, trimmed. `""` means the agent produced no
        /// visible answer at all.
        text: &'static str,
        /// Substring the reasoning channel must carry, when the agent streams
        /// reasoning. `None` means the agent sent no thoughts.
        thinking_contains: Option<&'static str>,
        stop_reason: StopReason,
        /// `None` means the agent's `PromptResponse` carried no `usage`.
        usage: Option<ExpectedUsage>,
        /// `(used, size)` from the agent's `usage_update` frame, or `None`
        /// for an agent that never reports its window. This is the only
        /// source of a context limit on a delegated session (A3), so the
        /// numbers are asserted, not just the shape.
        context_window: Option<(u64, u64)>,
    },
    /// The agent answered `session/prompt` with a JSON-RPC error, so the turn
    /// never produced a response. Substrings the surfaced error must contain.
    Failed {
        message_contains: &'static [&'static str],
    },
}

struct FixtureCase {
    /// Directory under `tests/fixtures/acp/recorded/`, and the value the
    /// fixture header must declare.
    agent: &'static str,
    /// `cwd` sent on `session/new` — mirrors what was recorded.
    cwd: &'static str,
    prompt: &'static str,
    /// `agentInfo` from the `initialize` response, as `(name, version)`.
    /// `None` for agents that omit it (it is optional in ACP 0.11).
    agent_info: Option<(&'static str, &'static str)>,
    /// `authMethods` ids from the `initialize` response, in order.
    auth_methods: &'static [&'static str],
    /// The exact session id the agent minted in the recording.
    session_id: &'static str,
    turn: TurnExpectation,
}

/// Claude Code 2.1.114 (`@zed-industries/claude-agent-acp` 0.22.2).
/// The plainest shape: answer text only, full usage including cache-write.
const CLAUDE: FixtureCase = FixtureCase {
    agent: "claude",
    cwd: "<HOME>/.crucible",
    prompt: "say hello in exactly 3 words",
    agent_info: Some(("@zed-industries/claude-agent-acp", "0.22.2")),
    auth_methods: &[],
    session_id: "c299d62f-4d2b-49d1-8b77-9c7f8d403f01",
    turn: TurnExpectation::Completed {
        // Three `agent_message_chunk` frames — `""`, `"Hello"`, `" to you!"` —
        // then the trailing `usage_update`. Real agents report the window at
        // the end of the turn; the mock in `turn_event_parity.rs` reports it
        // at the start, so both positions are covered.
        shapes: &[ChunkShape::Text, ChunkShape::ContextWindow],
        text: "Hello to you!",
        thinking_contains: None,
        stop_reason: StopReason::EndTurn,
        usage: Some(ExpectedUsage {
            prompt_tokens: 3,
            completion_tokens: 7,
            total_tokens: 22706,
            cache_read_tokens: Some(0),
            cache_creation_tokens: Some(22696),
        }),
        // `used` is 6 below the response's `totalTokens` (22706): it is the
        // context *before* the 7 output tokens landed, so the two numbers
        // measure the same quantity a moment apart.
        context_window: Some((22700, 1_000_000)),
    },
};

/// OpenCode 1.3.13. The only capture that streams reasoning, which makes it
/// the regression fixture for the `AgentThoughtChunk` arm: before it existed,
/// thoughts fell through to the terminal "ignoring session update" case and a
/// delegated turn rendered with no thinking block at all.
const OPENCODE: FixtureCase = FixtureCase {
    agent: "opencode",
    cwd: "<HOME>/.crucible",
    prompt: "say hello in exactly 3 words",
    agent_info: Some(("OpenCode", "1.3.13")),
    auth_methods: &["opencode-login"],
    session_id: "ses_257dac449ffeYWh0E1t42u4DA7",
    turn: TurnExpectation::Completed {
        shapes: &[
            ChunkShape::Thinking,
            ChunkShape::Text,
            ChunkShape::ContextWindow,
        ],
        text: "Hello there, friend!",
        thinking_contains: Some("exactly 3 words"),
        stop_reason: StopReason::EndTurn,
        // No `cachedWriteTokens` on the wire — the field stays `None` rather
        // than being zero-filled, so "not reported" stays distinguishable
        // from "reported as zero" (which is what Claude sends above).
        usage: Some(ExpectedUsage {
            prompt_tokens: 24496,
            completion_tokens: 54,
            total_tokens: 28278,
            cache_read_tokens: Some(3728),
            cache_creation_tokens: None,
        }),
        // Exactly inputTokens 24496 + cachedReadTokens 3728. The response's
        // totalTokens 28278 adds the 54 output tokens on top, which is why
        // the `PromptResponse` usage is preferred when both are present.
        context_window: Some((28224, 200_000)),
    },
};

/// cursor-agent. Recorded unauthenticated (`authMethods: ["cursor-login"]`),
/// so the turn ends in `Refusal` having streamed nothing. Thin, but it is the
/// only recorded capture of the produced-nothing path, which
/// `turn_stop_reason` collapses to `StopReason::Empty` downstream.
const CURSOR: FixtureCase = FixtureCase {
    agent: "cursor",
    cwd: "<HOME>/.crucible",
    prompt: "say hello in exactly 3 words",
    agent_info: None,
    auth_methods: &["cursor-login"],
    session_id: "38029559-7f8f-4b29-b552-20f461524096",
    turn: TurnExpectation::Completed {
        shapes: &[],
        text: "",
        thinking_contains: None,
        stop_reason: StopReason::Refusal,
        usage: None,
        // Reports neither usage nor a window: the delegated session that gets
        // no context indicator at all, and must not get a fabricated one.
        context_window: None,
    },
};

/// codex-acp 0.11.1. The handshake succeeds and `session/prompt` comes back as
/// a JSON-RPC error, so this is the recorded coverage for the mid-turn agent
/// error path.
///
/// Note what is asserted: Codex's `error.message` is the generic "Internal
/// error" and the whole reason the turn failed lives in the agent-defined
/// `error.data` — here a `message` holding a stringified upstream error
/// envelope whose innermost `message` names the unsupported model. The
/// surfaced text must carry that sentence, or the user is told only that
/// something internal went wrong.
const CODEX: FixtureCase = FixtureCase {
    agent: "codex",
    cwd: "<HOME>/.crucible",
    prompt: "say hello in exactly 3 words",
    agent_info: Some(("codex-acp", "0.11.1")),
    auth_methods: &["chatgpt", "codex-api-key", "openai-api-key"],
    session_id: "019da825-7c50-7133-8d2a-346b1c707f33",
    turn: TurnExpectation::Failed {
        message_contains: &[
            "Internal error",
            "-32603",
            "The 'gpt-5.2-codex' model is not supported when using Codex with a ChatGPT account.",
        ],
    },
};

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/acp/recorded")
        .join(rel)
}

async fn run_case(case: &FixtureCase) {
    let agent = case.agent;
    let path = fixture_path(&format!("{agent}/basic-chat.jsonl"));
    let fixture = ReplayFixture::load(&path)
        .unwrap_or_else(|e| panic!("[{agent}] load fixture {}: {e}", path.display()));

    assert_eq!(
        fixture.header.agent, agent,
        "[{agent}] fixture header agent"
    );
    let recorded_frames = fixture.records.len();

    let (writer, reader, driver) = fixture.into_transport();
    let driver_handle = tokio::spawn(driver);

    // `agent_path` is unused when a transport is supplied — nothing is spawned.
    let config = crucible_daemon::acp::client::ClientConfig {
        agent_path: PathBuf::from("/dev/null"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5_000),
        max_retries: None,
    };
    let mut client = CrucibleAcpClient::with_transport(config, writer, reader);

    // --- initialize ---------------------------------------------------------
    let init = client
        .initialize(InitializeRequest::new(1u16.into()))
        .await
        .unwrap_or_else(|e| panic!("[{agent}] initialize: {e}"));
    assert_eq!(
        init.protocol_version,
        1u16.into(),
        "[{agent}] protocol version"
    );

    let info = init
        .agent_info
        .as_ref()
        .map(|i| (i.name.as_str(), i.version.as_str()));
    assert_eq!(info, case.agent_info, "[{agent}] agentInfo");

    let auth: Vec<&str> = init
        .auth_methods
        .iter()
        .map(|m| m.id().0.as_ref())
        .collect();
    assert_eq!(auth, case.auth_methods, "[{agent}] authMethods");

    // --- session/new --------------------------------------------------------
    let session = client
        .create_new_session(NewSessionRequest::new(PathBuf::from(case.cwd)))
        .await
        .unwrap_or_else(|e| panic!("[{agent}] create session: {e}"));
    assert_eq!(
        session.session_id.0.as_ref(),
        case.session_id,
        "[{agent}] session id"
    );

    // --- session/prompt -----------------------------------------------------
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let request = PromptRequest::new(
        session.session_id.clone(),
        vec![ContentBlock::Text(TextContent::new(case.prompt))],
    );
    let result = client
        .send_prompt_with_callback(request, crucible_daemon::acp::channel_callback(tx))
        .await;

    let mut shapes = Vec::new();
    let mut thinking = String::new();
    let mut windows: Vec<(u64, u64)> = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        shapes.push(shape_of(&chunk));
        match chunk {
            StreamingChunk::Thinking(text) => thinking.push_str(&text),
            StreamingChunk::ContextWindow { used, limit } => windows.push((used, limit)),
            _ => {}
        }
    }
    let shapes = coalesce(&shapes);

    match &case.turn {
        TurnExpectation::Completed {
            shapes: expected_shapes,
            text: expected_text,
            thinking_contains,
            stop_reason,
            usage: expected_usage,
            context_window: expected_window,
        } => {
            let (text, tools, response) =
                result.unwrap_or_else(|e| panic!("[{agent}] send prompt: {e}"));

            assert_eq!(shapes, *expected_shapes, "[{agent}] streamed chunk shapes");
            assert_eq!(text.trim(), *expected_text, "[{agent}] reassembled answer");
            assert!(
                tools.is_empty(),
                "[{agent}] greeting turn should call no tools; got {tools:?}"
            );
            assert_eq!(response.stop_reason, *stop_reason, "[{agent}] stop reason");

            match thinking_contains {
                Some(needle) => {
                    assert!(
                        thinking.contains(needle),
                        "[{agent}] reasoning should contain {needle:?}; got {thinking:?}"
                    );
                    // Reasoning and answer are separate channels: a thought
                    // must never leak into the text the transcript stores.
                    assert!(
                        !text.contains(needle),
                        "[{agent}] reasoning leaked into the answer text: {text:?}"
                    );
                }
                None => assert!(
                    thinking.is_empty(),
                    "[{agent}] unexpected reasoning: {thinking:?}"
                ),
            }

            // A3. Collected as a list, not an Option: a second window frame
            // would silently overwrite the first, and an agent that reports
            // its window twice per turn is a different contract from one that
            // reports it once.
            assert_eq!(
                windows.as_slice(),
                expected_window.as_slice(),
                "[{agent}] context window reported by the agent"
            );

            let usage = client.take_last_usage();
            match (usage, expected_usage) {
                (Some(actual), Some(expected)) => {
                    assert_eq!(
                        actual.prompt_tokens, expected.prompt_tokens,
                        "[{agent}] prompt tokens"
                    );
                    assert_eq!(
                        actual.completion_tokens, expected.completion_tokens,
                        "[{agent}] completion tokens"
                    );
                    assert_eq!(
                        actual.total_tokens, expected.total_tokens,
                        "[{agent}] total tokens"
                    );
                    assert_eq!(
                        actual.cache_read_tokens, expected.cache_read_tokens,
                        "[{agent}] cache read tokens"
                    );
                    assert_eq!(
                        actual.cache_creation_tokens, expected.cache_creation_tokens,
                        "[{agent}] cache creation tokens"
                    );
                }
                (None, None) => {}
                (actual, expected) => panic!(
                    "[{agent}] usage mismatch: got {actual:?}, expected {}",
                    if expected.is_some() {
                        "Some(..)"
                    } else {
                        "None"
                    }
                ),
            }
        }
        TurnExpectation::Failed { message_contains } => {
            let err = result.expect_err(&format!("[{agent}] prompt should have failed"));
            let rendered = err.to_string();
            for needle in *message_contains {
                assert!(
                    rendered.contains(needle),
                    "[{agent}] error should mention {needle:?}; got {rendered:?}"
                );
            }
            assert!(
                client.take_last_usage().is_none(),
                "[{agent}] a failed turn reports no usage"
            );
        }
    }

    // --- replay hygiene -----------------------------------------------------
    drop(client);
    let outcome: ReplayOutcome = driver_handle.await.expect("driver panicked");
    assert!(
        outcome.is_clean(),
        "[{agent}] fixture replay diverged: {:?}",
        outcome.divergences
    );
    assert_eq!(
        outcome.frames_consumed, recorded_frames,
        "[{agent}] all fixture frames should be consumed during replay"
    );
}

#[tokio::test]
async fn claude_basic_chat_replays_cleanly() {
    run_case(&CLAUDE).await;
}

#[tokio::test]
async fn opencode_basic_chat_replays_cleanly() {
    run_case(&OPENCODE).await;
}

#[tokio::test]
async fn cursor_basic_chat_replays_cleanly() {
    run_case(&CURSOR).await;
}

#[tokio::test]
async fn codex_basic_chat_replays_cleanly() {
    run_case(&CODEX).await;
}

/// The `gemini` capture is a stub, not a turn: header plus a single outbound
/// `initialize` and nothing else — the agent never answered, so there is no
/// recorded behavior to replay. Driving it would only prove that a request
/// with no response times out, which is already covered by unit tests and
/// would cost 5s of wall clock per run.
///
/// This guards the file instead: it asserts it is still the stub we think it
/// is, so that a re-recording is noticed and promoted into the case table
/// rather than sitting unloaded. A useful re-recording needs, at minimum, an
/// `initialize` response (for `agentInfo`/`authMethods`), a `session/new`
/// response carrying a session id, and a `session/prompt` response with a
/// stop reason — i.e. the same handshake the other four captures have.
/// Re-record with `just record-acp-fixture gemini`.
#[test]
fn gemini_fixture_is_a_truncated_stub_not_yet_replayable() {
    let path = fixture_path("gemini/basic-chat.jsonl");
    let fixture = ReplayFixture::load(&path)
        .unwrap_or_else(|e| panic!("load fixture {}: {e}", path.display()));

    assert_eq!(fixture.header.agent, "gemini");

    let methods: Vec<&str> = fixture
        .records
        .iter()
        .filter_map(|r| r.frame.get("method").and_then(|m| m.as_str()))
        .collect();
    assert_eq!(
        methods,
        vec!["initialize"],
        "gemini capture is expected to be a lone outbound initialize; \
         if this now has a full turn, add it to the case table above"
    );
    assert_eq!(fixture.records.len(), 1, "gemini capture is a single frame");
    assert!(
        fixture
            .records
            .iter()
            .all(|r| r.frame.get("result").is_none() && r.frame.get("error").is_none()),
        "gemini capture is expected to contain no agent responses at all"
    );
}
