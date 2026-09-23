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
//! ## Why [`ChunkShape`] is spelled out here
//!
//! `acp_support::parity::chunk_kind` already projects a `StreamingChunk` onto
//! its variant name, which is exactly what [`shape_of`] below does — the two
//! are structural twins, differing only in returning `&'static str` versus an
//! enum. The duplication is not a design choice: `acp_support` is a
//! `#[path]`-included module tree, and this binary does not include it. Reaching
//! `chunk_kind` would mean pulling that tree into a test that needs none of the
//! rest of it. If a third `StreamingChunk` projection appears, or this binary
//! grows a reason to include `acp_support` anyway, collapse them.
//!
//! [`coalesce`] has no counterpart there and is the part that carries judgement:
//! chunk *boundaries* are an artifact of the agent's flush cadence, so only the
//! run-collapsed sequence is a contract.
//!
//! A different near-neighbour, `acp_support::parity`'s `ShapeProjector`/
//! `shapes()`, is genuinely inapplicable rather than merely out of reach: it
//! projects a `TurnEvent` stream, which is what `AcpAgentHandle` emits, and
//! `AcpAgentHandle::new` spawns a real agent process with no
//! transport-injection constructor — so a fixture cannot be driven through it
//! at all. This test sits one layer lower, on `CrucibleAcpClient`.

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, McpServer, NewSessionRequest, PromptRequest, StopReason,
    TextContent,
};
use agent_client_protocol::ByteStreams;
use crucible_daemon::acp::client::replay::{ReplayFixture, ReplayOutcome};
use crucible_daemon::acp::{CrucibleAcpClient, StreamingChunk};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

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
    ToolUpdate,
    ContextWindow,
}

fn shape_of(chunk: &StreamingChunk) -> ChunkShape {
    match chunk {
        StreamingChunk::Text(_) => ChunkShape::Text,
        StreamingChunk::Thinking(_) => ChunkShape::Thinking,
        StreamingChunk::ToolStart { .. } => ChunkShape::ToolStart,
        StreamingChunk::ToolEnd { .. } => ChunkShape::ToolEnd,
        StreamingChunk::ToolUpdate { .. } => ChunkShape::ToolUpdate,
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
        /// Every `(used, size)` the agent's `usage_update` frames carried,
        /// in order — empty for an agent that never reports its window. This
        /// is the only source of a context limit on a delegated session (A3),
        /// so the numbers are asserted, not just the shape. A list because
        /// agents differ in how often they report: claude-agent-acp sends
        /// four per turn and revises both operands as it goes.
        context_window: &'static [(u64, u64)],
        /// Substring the one tool call's `ToolEnd.result` must carry, or
        /// `None` for a turn with no tool calls. Hermes puts a polished
        /// tool's result only in `content` text blocks (`rawOutput` is
        /// absent), so this asserts the content path fills the result.
        tool_result_contains: Option<&'static str>,
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
    /// Basename of the capture under that directory, without `.jsonl`. One
    /// agent can have several: re-recording against a newer build replaces
    /// what the agent does *now*, and the path it no longer takes — a prompt
    /// error, a refusal — stays behind as its own scenario rather than being
    /// deleted with the file.
    scenario: &'static str,
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

/// Claude Code via `@agentclientprotocol/claude-agent-acp` 0.73.0.
/// The plainest shape: answer text only, full usage including cache-write.
///
/// Four `usage_update` frames, which is why the window is a list. The agent
/// revises both operands mid-turn — `size` moves from 200000 to 1000000 on
/// the last one — so a single-value expectation would pin whichever frame
/// happened to arrive last and call the rest noise.
const CLAUDE: FixtureCase = FixtureCase {
    agent: "claude",
    scenario: "basic-chat",
    cwd: "<HOME>/.crucible/workspaces/chat-2026-09-04T0223-qsogzw",
    prompt: "say hello in exactly 3 words",
    agent_info: Some(("@agentclientprotocol/claude-agent-acp", "0.73.0")),
    auth_methods: &[],
    session_id: "4d5cb397-557c-4d1c-b5b9-3bcb291fcebe",
    turn: TurnExpectation::Completed {
        // The window lands before the text here: this bridge reports usage as
        // soon as the turn starts, where opencode reports it at the end. Both
        // orders are therefore covered by recorded captures.
        shapes: &[
            ChunkShape::ContextWindow,
            ChunkShape::Text,
            ChunkShape::ContextWindow,
        ],
        text: "Hello there, friend.",
        thinking_contains: None,
        stop_reason: StopReason::EndTurn,
        usage: Some(ExpectedUsage {
            prompt_tokens: 2,
            completion_tokens: 10,
            total_tokens: 27509,
            cache_read_tokens: Some(10346),
            cache_creation_tokens: Some(17151),
        }),
        context_window: &[
            (27503, 200_000),
            (27509, 200_000),
            (27509, 200_000),
            (27509, 1_000_000),
        ],
        tool_result_contains: None,
    },
};

/// OpenCode 1.3.13. The only capture that streams reasoning, which makes it
/// the regression fixture for the `AgentThoughtChunk` arm: before it existed,
/// thoughts fell through to the terminal "ignoring session update" case and a
/// delegated turn rendered with no thinking block at all.
const OPENCODE: FixtureCase = FixtureCase {
    agent: "opencode",
    scenario: "basic-chat",
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
        context_window: &[(28224, 200_000)],
        tool_result_contains: None,
    },
};

/// cursor-agent's own ACP server, authenticated. Streams reasoning and then
/// the answer, and reports neither usage nor a context window — the
/// delegated session that gets no context indicator at all and must not be
/// given a fabricated one.
const CURSOR: FixtureCase = FixtureCase {
    agent: "cursor",
    scenario: "basic-chat",
    cwd: "<HOME>/.crucible/workspaces/chat-2026-09-04T0223-6oxna7",
    prompt: "say hello in exactly 3 words",
    agent_info: None,
    auth_methods: &["cursor_login"],
    session_id: "388e3149-7196-48ee-8a47-d028dee22444",
    turn: TurnExpectation::Completed {
        shapes: &[ChunkShape::Thinking, ChunkShape::Text],
        text: "Hello there friend",
        thinking_contains: Some("three-word"),
        stop_reason: StopReason::EndTurn,
        usage: None,
        context_window: &[],
        tool_result_contains: None,
    },
};

/// The same agent before `cursor-agent login`, kept from the capture that
/// preceded the authenticated one above. `authMethods` names the login it
/// wants and the turn ends in `Refusal` having streamed nothing. It is the
/// only recorded capture of the produced-nothing path, which
/// `turn_stop_reason` collapses to `StopReason::Empty` downstream, so it
/// outlives the re-recording that replaced `basic-chat`.
///
/// Note the auth id: this capture says `cursor-login`, the live agent now
/// says `cursor_login`. Ids are opaque and agents rename them.
const CURSOR_UNAUTHENTICATED: FixtureCase = FixtureCase {
    agent: "cursor",
    scenario: "unauthenticated",
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
        context_window: &[],
        tool_result_contains: None,
    },
};

/// codex-acp 1.8.0, authenticated and answering normally. Seven one-word
/// `agent_message_chunk` frames, so this is also the capture that proves
/// chunk reassembly across many small frames.
const CODEX: FixtureCase = FixtureCase {
    agent: "codex",
    scenario: "basic-chat",
    cwd: "<HOME>/.crucible/workspaces/chat-2026-09-04T0223-5wxycb",
    prompt: "say hello in exactly 3 words",
    agent_info: Some(("@agentclientprotocol/codex-acp", "1.8.0")),
    auth_methods: &["api-key", "chat-gpt"],
    session_id: "01a06a3a-bcf2-74c2-aa4e-e8ac64bea23b",
    turn: TurnExpectation::Completed {
        shapes: &[ChunkShape::Text, ChunkShape::ContextWindow],
        text: "Hello, good to meet you.",
        thinking_contains: None,
        stop_reason: StopReason::EndTurn,
        usage: Some(ExpectedUsage {
            prompt_tokens: 6321,
            completion_tokens: 11,
            total_tokens: 17340,
            cache_read_tokens: Some(11008),
            cache_creation_tokens: None,
        }),
        context_window: &[(17340, 258_400)],
        tool_result_contains: None,
    },
};

/// codex-acp 0.11.1, kept from the capture that preceded the working one
/// above. The handshake succeeds and `session/prompt` comes back as a
/// JSON-RPC error, so this is the recorded coverage for the mid-turn agent
/// error path — the reason it outlives the re-recording that replaced
/// `basic-chat`.
///
/// Note what is asserted: Codex's `error.message` is the generic "Internal
/// error" and the whole reason the turn failed lives in the agent-defined
/// `error.data` — here a `message` holding a stringified upstream error
/// envelope whose innermost `message` names the unsupported model. The
/// surfaced text must carry that sentence, or the user is told only that
/// something internal went wrong.
const CODEX_PROMPT_ERROR: FixtureCase = FixtureCase {
    agent: "codex",
    scenario: "prompt-error",
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

/// Hermes 0.20.5. Not a live capture: the frames are built one by one from
/// the `acp_adapter` sources at commit `f293e720` (see the fixture header).
/// Re-record with `just record-acp-fixture hermes` when a live Hermes is
/// available, and keep one polished tool in the turn.
///
/// The turn runs the polished `terminal` tool. Hermes sends the result only
/// in `content` text blocks — `rawOutput` is absent, and the completion
/// frame carries no `title`. The case therefore pins the two Hermes-shaped
/// behaviours: the result reaches `ToolEnd` through the content path, and
/// the call keeps the name its own `tool_call` announced.
const HERMES: FixtureCase = FixtureCase {
    agent: "hermes",
    scenario: "basic-chat",
    cwd: "<HOME>/.crucible",
    prompt: "run pwd and tell me the directory",
    agent_info: Some(("hermes-agent", "0.20.5")),
    auth_methods: &["hermes-setup"],
    session_id: "6f0d2b9e-8c47-4a53-b1a9-2e7d4c5f8a10",
    turn: TurnExpectation::Completed {
        shapes: &[
            ChunkShape::ToolStart,
            ChunkShape::ToolEnd,
            ChunkShape::Text,
            ChunkShape::ContextWindow,
        ],
        text: "The working directory is <HOME>/.crucible.",
        thinking_contains: None,
        stop_reason: StopReason::EndTurn,
        // Hermes reports `thoughtTokens` and `cachedReadTokens` only when
        // the run counted them; this turn reports the three base numbers.
        usage: Some(ExpectedUsage {
            prompt_tokens: 5210,
            completion_tokens: 74,
            total_tokens: 5284,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        }),
        context_window: &[(6100, 128_000)],
        tool_result_contains: Some("<HOME>/.crucible"),
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

/// The params of the first outgoing `method` frame in the fixture.
fn recorded_params<'a>(fixture: &'a ReplayFixture, method: &str) -> &'a serde_json::Value {
    fixture
        .records
        .iter()
        .find(|record| record.frame.get("method").and_then(|m| m.as_str()) == Some(method))
        .and_then(|record| record.frame.get("params"))
        .unwrap_or_else(|| panic!("the fixture records no {method} request"))
}

async fn run_case(case: &FixtureCase) {
    let agent = case.agent;
    let path = fixture_path(&format!("{agent}/{}.jsonl", case.scenario));
    let fixture = ReplayFixture::load(&path)
        .unwrap_or_else(|e| panic!("[{agent}] load fixture {}: {e}", path.display()));

    assert_eq!(
        fixture.header.agent, agent,
        "[{agent}] fixture header agent"
    );
    let recorded_frames = fixture.records.len();

    // The replay compares what the client sends against what was recorded,
    // so the test sends what the daemon sent then: the same MCP servers and
    // the same prompt text. The daemon put retrieved context in front of the
    // user's words, so the recorded text ends with `case.prompt` rather than
    // equalling it.
    let mcp_servers: Vec<McpServer> =
        serde_json::from_value(recorded_params(&fixture, "session/new")["mcpServers"].clone())
            .unwrap_or_else(|e| panic!("[{agent}] recorded mcpServers parse: {e}"));
    let prompt_text: String = recorded_params(&fixture, "session/prompt")["prompt"]
        .as_array()
        .unwrap_or_else(|| panic!("[{agent}] the recorded prompt has no blocks"))
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect();
    assert!(
        prompt_text.ends_with(case.prompt),
        "[{agent}] the recorded prompt must end with the user's words {:?}",
        case.prompt
    );

    let (writer, reader, driver) = fixture.into_transport();
    let driver_handle = tokio::spawn(driver);

    // `agent_path` is unused when a transport is supplied — nothing is spawned.
    let config = crucible_daemon::acp::client::ClientConfig {
        agent_path: PathBuf::from("/dev/null"),
        agent_args: None,
        timeout_ms: Some(5_000),
        ..Default::default()
    };
    let client = CrucibleAcpClient::connect(
        config,
        ByteStreams::new(writer.compat_write(), reader.compat()),
        agent,
        None,
    )
    .await
    .unwrap_or_else(|e| panic!("[{agent}] connect: {e}"));

    // --- initialize ---------------------------------------------------------
    let init = client
        .request(InitializeRequest::new(1u16.into()))
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
        .request(NewSessionRequest::new(PathBuf::from(case.cwd)).mcp_servers(mcp_servers))
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
        vec![ContentBlock::Text(TextContent::new(prompt_text))],
    );
    let result = client.prompt(request, &tx).await;

    let mut shapes = Vec::new();
    let mut text = String::new();
    let mut thinking = String::new();
    let mut windows: Vec<(u64, u64)> = Vec::new();
    let mut tool_ends: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        shapes.push(shape_of(&chunk));
        match chunk {
            StreamingChunk::Text(chunk_text) => text.push_str(&chunk_text),
            StreamingChunk::Thinking(chunk_text) => thinking.push_str(&chunk_text),
            StreamingChunk::ContextWindow { used, limit } => windows.push((used, limit)),
            StreamingChunk::ToolEnd {
                name,
                result,
                error,
                ..
            } => tool_ends.push((name, result, error)),
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
            tool_result_contains,
        } => {
            let (summary, response) =
                result.unwrap_or_else(|e| panic!("[{agent}] send prompt: {e}"));

            assert_eq!(shapes, *expected_shapes, "[{agent}] streamed chunk shapes");
            assert_eq!(text.trim(), *expected_text, "[{agent}] reassembled answer");
            assert_eq!(
                summary.announced_any,
                tool_result_contains.is_some(),
                "[{agent}] the summary must say whether the turn called tools; got {summary:?}"
            );
            match tool_result_contains {
                Some(needle) => {
                    assert_eq!(tool_ends.len(), 1, "[{agent}] expected one ToolEnd");
                    let (name, result, error) = &tool_ends[0];
                    assert!(!name.is_empty(), "[{agent}] ToolEnd must carry a name");
                    let result = result
                        .as_deref()
                        .unwrap_or_else(|| panic!("[{agent}] ToolEnd carried no result"));
                    assert!(
                        result.contains(needle),
                        "[{agent}] tool result should contain {needle:?}; got {result:?}"
                    );
                    assert_eq!(error, &None, "[{agent}] the tool did not fail");
                }
                None => assert!(
                    tool_ends.is_empty(),
                    "[{agent}] unexpected tool calls: {tool_ends:?}"
                ),
            }
            assert_eq!(
                summary.produced_content,
                !expected_text.is_empty(),
                "[{agent}] the summary must say whether the turn showed text; got {summary:?}"
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
                *expected_window,
                "[{agent}] context window reported by the agent"
            );

            let usage = crucible_daemon::acp::turn_usage(&response);
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
async fn cursor_unauthenticated_replays_cleanly() {
    run_case(&CURSOR_UNAUTHENTICATED).await;
}

#[tokio::test]
async fn codex_basic_chat_replays_cleanly() {
    run_case(&CODEX).await;
}

#[tokio::test]
async fn codex_prompt_error_replays_cleanly() {
    run_case(&CODEX_PROMPT_ERROR).await;
}

#[tokio::test]
async fn hermes_basic_chat_replays_cleanly() {
    run_case(&HERMES).await;
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

/// The frames in `tests/fixtures/acp/tool_frames/` come from adapter source,
/// not from a live capture. This test makes sure that each frame decodes as
/// the SDK type that the client decodes it as.
#[test]
fn each_tool_frame_decodes_as_its_sdk_type() {
    use agent_client_protocol::schema::v1::{
        RequestPermissionRequest, SessionNotification, SessionUpdate,
    };

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp/tool_frames");
    for entry in std::fs::read_dir(&dir).expect("the tool_frames directory exists") {
        let path = entry.expect("a directory entry").path();
        let text = std::fs::read_to_string(&path).expect("the fixture reads");
        // The first line is the header that names the source.
        for (n, line) in text.lines().enumerate().skip(1) {
            let at = format!("{}:{}", path.display(), n + 1);
            let record: serde_json::Value = serde_json::from_str(line).expect(&at);
            let params = record["frame"]["params"].clone();
            match record["frame"]["method"].as_str() {
                Some("session/update") => {
                    let note: SessionNotification = serde_json::from_value(params).expect(&at);
                    assert!(
                        matches!(
                            note.update,
                            SessionUpdate::ToolCall(_) | SessionUpdate::ToolCallUpdate(_)
                        ),
                        "{at}: not a tool frame"
                    );
                }
                Some("session/request_permission") => {
                    let _: RequestPermissionRequest = serde_json::from_value(params).expect(&at);
                }
                other => panic!("{at}: unexpected method {other:?}"),
            }
        }
    }
}

/// The ACP config after a boot that runs the shipped
/// `runtime/defaults/init.luau` of this repository, then `init_lua`.
async fn boot_acp_config(init_lua: &str) -> crucible_core::config::AcpConfig {
    use crucible_daemon::daemon_plugins::{evaluate_boot_config_with_paths, PluginPathsFn};

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("init.lua"), init_lua).unwrap();
    let no_plugins: PluginPathsFn = std::sync::Arc::new(|_| Vec::new());
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    let boot = evaluate_boot_config_with_paths(
        Some(tmp.path().join("config.toml")),
        None,
        None,
        no_plugins,
        vec![runtime],
    )
    .await
    .expect("the boot evaluates");
    assert_eq!(boot.eval_error, None, "init.lua evaluates");
    boot.config.acp
}

/// The key table of `agent`, as the agent profile resolves it.
fn key_table(
    acp: &crucible_core::config::AcpConfig,
    agent: &str,
) -> Vec<crucible_core::types::AgentKeys> {
    crucible_daemon::acp::discovery::profile(agent, acp)
        .expect("the profile resolves")
        .expect("the agent is known")
        .tools
}

/// Classify each frame in one tool_frames fixture. Each item is the line
/// number and the typed fields of the canonical call, without `raw`,
/// `primary` and `diffs`.
fn classify_fixture(
    agent: &str,
    table: &[crucible_core::types::AgentKeys],
) -> Vec<(usize, serde_json::Value)> {
    use agent_client_protocol::schema::v1::{
        RequestPermissionRequest, SessionNotification, SessionUpdate,
    };
    use crucible_core::types::{classify_acp, RawToolCall};

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/acp/tool_frames")
        .join(format!("{agent}.jsonl"));
    let text = std::fs::read_to_string(&path).expect("the fixture reads");
    text.lines()
        .enumerate()
        .skip(1)
        .map(|(n, line)| {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            let params = record["frame"]["params"].clone();
            let raw = match record["frame"]["method"].as_str() {
                Some("session/update") => {
                    match serde_json::from_value::<SessionNotification>(params)
                        .unwrap()
                        .update
                    {
                        SessionUpdate::ToolCall(c) => RawToolCall::from(&c),
                        SessionUpdate::ToolCallUpdate(u) => RawToolCall::from(&u),
                        other => panic!("not a tool frame: {other:?}"),
                    }
                }
                _ => RawToolCall::from(
                    &serde_json::from_value::<RequestPermissionRequest>(params)
                        .unwrap()
                        .tool_call,
                ),
            };
            let mut v = serde_json::to_value(classify_acp(raw, table)).unwrap();
            let fields = v.as_object_mut().unwrap();
            fields.remove("raw");
            fields.remove("primary");
            fields.remove("diffs");
            (n + 1, v)
        })
        .collect()
}

/// The canonical call that the default matcher gives for each frame, with
/// the key table of the agent. A line of a status-only update gives the
/// fallback `tool`, because the frame names nothing. A call that nothing
/// names gets its kind as its name. A command or file kind
/// with no command line or no path also gives `tool`. Step 4 merges updates
/// into one call, so these rows show one frame each.
const EXPECTED_CLASSES: &[(&str, usize, &str)] = &[
    ("claude", 2, r#"{"kind":"tool","tool":"Bash"}"#),
    (
        "claude",
        3,
        r#"{"kind":"command","tool":"command","command":"ls src"}"#,
    ),
    (
        "claude",
        4,
        r#"{"kind":"command","tool":"command","command":"ls src"}"#,
    ),
    (
        "claude",
        5,
        r#"{"kind":"command","tool":"Bash","command":"ls src"}"#,
    ),
    ("claude", 6, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 7, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 8, r#"{"kind":"tool","tool":"Edit"}"#),
    (
        "claude",
        9,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/src/lib.rs"]}"#,
    ),
    (
        "claude",
        10,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/src/lib.rs"]}"#,
    ),
    (
        "claude",
        11,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/src/lib.rs"]}"#,
    ),
    (
        "claude",
        12,
        r#"{"kind":"file_edit","tool":"Edit","paths":["/home/user/proj/src/lib.rs"]}"#,
    ),
    ("claude", 13, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "claude",
        14,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/src/lib.rs"]}"#,
    ),
    ("claude", 15, r#"{"kind":"tool","tool":"Read"}"#),
    (
        "claude",
        16,
        r#"{"kind":"file_read","tool":"file_read","paths":["/etc/hosts"]}"#,
    ),
    (
        "claude",
        17,
        r#"{"kind":"file_read","tool":"Read","paths":["/etc/hosts"]}"#,
    ),
    ("claude", 18, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 19, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "claude",
        20,
        r#"{"kind":"mcp_tool","tool":"mcp__srv__tool"}"#,
    ),
    (
        "claude",
        21,
        r#"{"kind":"mcp_tool","tool":"mcp__srv__tool"}"#,
    ),
    (
        "claude",
        22,
        r#"{"kind":"mcp_tool","tool":"mcp__srv__tool"}"#,
    ),
    ("claude", 23, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 24, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 25, r#"{"kind":"fetch","tool":"WebSearch"}"#),
    (
        "claude",
        26,
        r#"{"kind":"search","tool":"search","query":"acp spec"}"#,
    ),
    (
        "claude",
        27,
        r#"{"kind":"search","tool":"WebSearch","query":"acp spec"}"#,
    ),
    ("claude", 28, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 29, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 30, r#"{"kind":"fetch","tool":"WebFetch"}"#),
    (
        "claude",
        31,
        r#"{"kind":"fetch","tool":"fetch","url":"https://example.com"}"#,
    ),
    (
        "claude",
        32,
        r#"{"kind":"fetch","tool":"fetch","url":"https://example.com"}"#,
    ),
    (
        "claude",
        33,
        r#"{"kind":"fetch","tool":"WebFetch","url":"https://example.com"}"#,
    ),
    ("claude", 34, r#"{"kind":"tool","tool":"tool"}"#),
    ("claude", 35, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "codex-rust",
        2,
        r#"{"kind":"command","tool":"command","command":"cargo test"}"#,
    ),
    (
        "codex-rust",
        3,
        r#"{"kind":"command","tool":"command","command":"cargo test"}"#,
    ),
    ("codex-rust", 4, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "codex-rust",
        5,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    (
        "codex-rust",
        6,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    (
        "codex-rust",
        7,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    (
        "codex-rust",
        8,
        r#"{"kind":"file_read","tool":"file_read","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    ("codex-rust", 9, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "codex-rust",
        10,
        r#"{"kind":"mcp_tool","tool":"search_notes","query":"rust"}"#,
    ),
    ("codex-rust", 11, r#"{"kind":"mcp_tool","tool":"mcp_tool"}"#),
    ("codex-rust", 12, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-rust", 13, r#"{"kind":"fetch","tool":"fetch"}"#),
    (
        "codex-rust",
        14,
        r#"{"kind":"search","tool":"search","query":"rust acp"}"#,
    ),
    ("codex-rust", 15, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-rust", 16, r#"{"kind":"fetch","tool":"fetch"}"#),
    (
        "codex-rust",
        17,
        r#"{"kind":"fetch","tool":"fetch","url":"https://agentclientprotocol.com","query":"https://agentclientprotocol.com"}"#,
    ),
    ("codex-rust", 18, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "codex-ts",
        2,
        r#"{"kind":"command","tool":"command","command":"cargo test"}"#,
    ),
    (
        "codex-ts",
        3,
        r#"{"kind":"command","tool":"command","command":"cargo test"}"#,
    ),
    ("codex-ts", 4, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-ts", 5, r#"{"kind":"tool","tool":"exec_command"}"#),
    (
        "codex-ts",
        6,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    (
        "codex-ts",
        7,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    ("codex-ts", 8, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "codex-ts",
        9,
        r#"{"kind":"file_read","tool":"exec_command","paths":["/home/user/project/src/lib.rs"]}"#,
    ),
    ("codex-ts", 10, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-ts", 11, r#"{"kind":"tool","tool":"exec_command"}"#),
    (
        "codex-ts",
        12,
        r#"{"kind":"search","tool":"search_notes","query":"rust"}"#,
    ),
    ("codex-ts", 13, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-ts", 14, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-ts", 15, r#"{"kind":"tool","tool":"tool"}"#),
    ("codex-ts", 16, r#"{"kind":"search","tool":"search"}"#),
    (
        "codex-ts",
        17,
        r#"{"kind":"search","tool":"search","query":"rust acp"}"#,
    ),
    ("codex-ts", 18, r#"{"kind":"search","tool":"search"}"#),
    (
        "codex-ts",
        19,
        r#"{"kind":"fetch","tool":"fetch","url":"https://agentclientprotocol.com","query":"https://agentclientprotocol.com"}"#,
    ),
    (
        "gemini",
        2,
        r#"{"kind":"command","tool":"command","command":"ls -la src"}"#,
    ),
    (
        "gemini",
        3,
        r#"{"kind":"command","tool":"command","command":"ls -la src"}"#,
    ),
    (
        "gemini",
        4,
        r#"{"kind":"command","tool":"command","command":"ls -la src"}"#,
    ),
    (
        "gemini",
        5,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/config.py"]}"#,
    ),
    (
        "gemini",
        6,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/config.py"]}"#,
    ),
    (
        "gemini",
        7,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/home/user/proj/config.py"]}"#,
    ),
    (
        "gemini",
        8,
        r#"{"kind":"file_read","tool":"file_read","paths":["/home/user/proj/src/main.rs"]}"#,
    ),
    (
        "gemini",
        9,
        r#"{"kind":"file_read","tool":"file_read","paths":["/home/user/proj/src/main.rs"]}"#,
    ),
    ("gemini", 10, r#"{"kind":"tool","tool":"tool"}"#),
    ("gemini", 11, r#"{"kind":"tool","tool":"tool"}"#),
    ("gemini", 12, r#"{"kind":"tool","tool":"tool"}"#),
    ("gemini", 13, r#"{"kind":"search","tool":"search"}"#),
    ("gemini", 14, r#"{"kind":"search","tool":"search"}"#),
    ("gemini", 15, r#"{"kind":"fetch","tool":"fetch"}"#),
    ("gemini", 16, r#"{"kind":"fetch","tool":"fetch"}"#),
    ("gemini", 17, r#"{"kind":"fetch","tool":"fetch"}"#),
    (
        "antigravity",
        2,
        r#"{"kind":"command","tool":"command","command":"cargo test -p app"}"#,
    ),
    (
        "antigravity",
        3,
        r#"{"kind":"command","tool":"command","command":"cargo test -p app"}"#,
    ),
    (
        "antigravity",
        4,
        r#"{"kind":"command","tool":"command","command":"cargo test -p app"}"#,
    ),
    ("antigravity", 5, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "antigravity",
        6,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/work/app/src/lib.rs"]}"#,
    ),
    (
        "antigravity",
        7,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/work/app/src/lib.rs"]}"#,
    ),
    (
        "antigravity",
        8,
        r#"{"kind":"file_edit","tool":"file_edit","paths":["/work/app/src/lib.rs"]}"#,
    ),
    ("antigravity", 9, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "antigravity",
        10,
        r#"{"kind":"file_read","tool":"file_read","paths":["/work/app/src/main.rs"]}"#,
    ),
    ("antigravity", 11, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "antigravity",
        12,
        r#"{"kind":"mcp_tool","tool":"github_get_issue"}"#,
    ),
    (
        "antigravity",
        13,
        r#"{"kind":"mcp_tool","tool":"github_get_issue"}"#,
    ),
    (
        "antigravity",
        14,
        r#"{"kind":"mcp_tool","tool":"github_get_issue"}"#,
    ),
    ("antigravity", 15, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "antigravity",
        16,
        r#"{"kind":"search","tool":"search","query":"acp tool call schema"}"#,
    ),
    (
        "antigravity",
        17,
        r#"{"kind":"search","tool":"search","query":"acp tool call schema"}"#,
    ),
    (
        "antigravity",
        18,
        r#"{"kind":"search","tool":"search","query":"acp tool call schema"}"#,
    ),
    ("antigravity", 19, r#"{"kind":"tool","tool":"tool"}"#),
    (
        "antigravity",
        20,
        r#"{"kind":"fetch","tool":"fetch","url":"https://agentclientprotocol.com/protocol/tool-calls"}"#,
    ),
    (
        "antigravity",
        21,
        r#"{"kind":"fetch","tool":"fetch","url":"https://agentclientprotocol.com/protocol/tool-calls"}"#,
    ),
    (
        "antigravity",
        22,
        r#"{"kind":"fetch","tool":"fetch","url":"https://agentclientprotocol.com/protocol/tool-calls"}"#,
    ),
    ("antigravity", 23, r#"{"kind":"tool","tool":"tool"}"#),
];

/// The fixtures run through the key tables of the shipped Lua defaults. Both
/// codex fixtures use the one table of the built-in `codex`.
#[tokio::test]
async fn each_tool_frame_gives_its_canonical_call() {
    let acp = boot_acp_config("").await;
    let mut want = EXPECTED_CLASSES.iter();
    for (agent, profile) in [
        ("claude", "claude"),
        ("codex-rust", "codex"),
        ("codex-ts", "codex"),
        ("gemini", "gemini"),
        ("antigravity", "antigravity"),
    ] {
        for (line, got) in classify_fixture(agent, &key_table(&acp, profile)) {
            let (a, l, fields) = want.next().expect("a row for each frame");
            assert_eq!((*a, *l), (agent, line), "the table follows the fixtures");
            let fields: serde_json::Value = serde_json::from_str(fields).unwrap();
            assert_eq!(got, fields, "{agent}.jsonl:{line}");
        }
    }
    assert!(want.next().is_none(), "each row names a frame");
}

/// A table in the user's init.lua replaces the shipped table of that agent.
/// The other agents keep their shipped tables.
#[tokio::test]
async fn a_user_key_table_replaces_the_shipped_one() {
    use crucible_core::types::AgentKeys;

    let shipped = boot_acp_config("").await;
    assert!(
        !key_table(&shipped, "gemini").is_empty(),
        "gemini ships a table"
    );

    let acp = boot_acp_config(
        r#"cru.config.set { acp = { agents = { gemini = { tools = {
            { title = "Run ", tool = { "/title" } },
        } } } } }"#,
    )
    .await;
    let want = AgentKeys {
        title: Some("Run ".into()),
        tool: vec!["/title".into()],
        ..AgentKeys::default()
    };
    assert_eq!(key_table(&acp, "gemini"), vec![want]);
    assert_eq!(key_table(&acp, "codex"), key_table(&shipped, "codex"));
}

/// What one turn over a tool_frames case showed: the canonical call of each
/// permission request, and each chunk of the turn.
struct JoinedTurn {
    asked: Vec<crucible_core::types::CanonicalToolCall>,
    chunks: Vec<StreamingChunk>,
}

impl JoinedTurn {
    /// The canonical call of each `ToolStart`, in stream order.
    fn started(&self) -> Vec<&crucible_core::types::CanonicalToolCall> {
        self.chunks
            .iter()
            .filter_map(|chunk| match chunk {
                StreamingChunk::ToolStart { call, .. } => Some(call),
                _ => None,
            })
            .collect()
    }
}

/// Run one turn of a real client over the frames of `case` in the
/// tool_frames fixture of `agent`, in fixture order, with the key table of
/// `profile`. The agent waits for the answer to each permission request
/// before it sends the next frame, as both codex adapters do.
async fn joined_turn(agent: &str, profile: &str, case: &str) -> JoinedTurn {
    use agent_client_protocol::schema::v1::{
        PermissionOptionKind, RequestPermissionOutcome, SelectedPermissionOutcome,
    };
    use crucible_daemon::acp::client::{ClientConfig, PermissionRequestHandler};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/acp/tool_frames")
        .join(format!("{agent}.jsonl"));
    let frames: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .expect("the fixture reads")
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|record| record["case"] == case)
        .map(|record| record["frame"].clone())
        .collect();
    assert!(!frames.is_empty(), "{agent}.jsonl has the case {case}");

    let asked: Arc<Mutex<Vec<crucible_core::types::CanonicalToolCall>>> = Arc::default();
    let recorder = Arc::clone(&asked);
    let permission: PermissionRequestHandler = Arc::new(move |call, options| {
        recorder.lock().unwrap().push(call);
        let allow = options
            .iter()
            .find(|o| o.kind == PermissionOptionKind::AllowOnce)
            .expect("the agent offers allow_once")
            .option_id
            .clone();
        Box::pin(async move {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(allow))
        })
    });

    let (client_end, agent_end) = tokio::io::duplex(256 * 1024);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, mut agent_write) = tokio::io::split(agent_end);
    let config = ClientConfig {
        tools: key_table(&boot_acp_config("").await, profile),
        ..ClientConfig::default()
    };
    let client = CrucibleAcpClient::connect(
        config,
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
        agent,
        Some(permission),
    )
    .await
    .expect("the client connects");

    let (out, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel();
    let turn = client.prompt(
        PromptRequest::new("sess-1".to_string(), vec![ContentBlock::from("go")]),
        &out,
    );
    let agent_side = async move {
        let mut lines = BufReader::new(agent_read).lines();
        let prompt: serde_json::Value = serde_json::from_str(
            &lines
                .next_line()
                .await
                .unwrap()
                .expect("the client sends the prompt"),
        )
        .unwrap();
        for frame in frames {
            let is_request = frame.get("id").is_some();
            agent_write
                .write_all(format!("{frame}\n").as_bytes())
                .await
                .unwrap();
            if is_request {
                let reply: serde_json::Value = serde_json::from_str(
                    &lines
                        .next_line()
                        .await
                        .unwrap()
                        .expect("the client answers"),
                )
                .unwrap();
                assert!(reply.get("result").is_some(), "the client answers: {reply}");
            }
        }
        let done = serde_json::json!({
            "jsonrpc": "2.0", "id": prompt["id"], "result": {"stopReason": "end_turn"}
        });
        agent_write
            .write_all(format!("{done}\n").as_bytes())
            .await
            .unwrap();
    };
    let (result, ()) = tokio::join!(turn, agent_side);
    result.expect("the turn ends");
    drop(out);

    let mut chunks = Vec::new();
    while let Ok(chunk) = chunk_rx.try_recv() {
        chunks.push(chunk);
    }
    let asked = asked.lock().unwrap().clone();
    JoinedTurn { asked, chunks }
}

/// The Rust codex adapter asks before it sends the `tool_call`. The request
/// is decided on its own fields, and the call that follows joins the same
/// entry: one card, with the diff and the command of the request.
#[tokio::test]
async fn an_old_codex_request_before_its_tool_call_joins_one_entry() {
    let turn = joined_turn("codex-rust", "codex", "file_edit").await;

    let [asked] = turn.asked.as_slice() else {
        panic!("one permission request, got {:?}", turn.asked)
    };
    assert_eq!(asked.kind, "file_edit");
    assert_eq!(asked.diffs.len(), 1, "the request carries its own diff");

    let [started] = turn.started()[..] else {
        panic!("one card for one toolCallId, got {:?}", turn.chunks)
    };
    assert_eq!(started.kind, "file_edit");
    assert_eq!(started.paths, ["/home/user/project/src/lib.rs"]);
    assert_eq!(started.diffs.len(), 1);

    let shell = joined_turn("codex-rust", "codex", "shell").await;
    assert_eq!(shell.asked[0].command.as_deref(), Some("cargo test"));
    let [started] = shell.started()[..] else {
        panic!("one card for one toolCallId, got {:?}", shell.chunks)
    };
    assert_eq!(started.command.as_deref(), Some("cargo test"));
}

/// The TypeScript codex adapter sends the diff only in the `tool_call`. Its
/// permission request has no diff, so the request gets the diff of the
/// earlier frame (rule 5).
#[tokio::test]
async fn a_new_codex_request_with_no_diff_gets_the_diff_of_its_tool_call() {
    let turn = joined_turn("codex-ts", "codex", "file_edit").await;

    let [asked] = turn.asked.as_slice() else {
        panic!("one permission request, got {:?}", turn.asked)
    };
    assert_eq!(asked.kind, "file_edit");
    assert_eq!(asked.paths, ["/home/user/project/src/lib.rs"]);
    let [diff] = asked.diffs.as_slice() else {
        panic!("the request joins the diff of its tool_call: {asked:?}")
    };
    assert_eq!(diff.new_content, "fn b() {}\nfn c() {}\n");
    assert_eq!(
        asked.raw.as_ref().and_then(|raw| raw.title.as_deref()),
        Some("Edit files"),
        "a field that the request sets wins"
    );
}

/// The TypeScript codex adapter asks about an MCP call with `kind` alone.
/// The tool comes from the `tool_call` before it, and the card shows the
/// same canonical tool.
#[tokio::test]
async fn a_new_codex_mcp_request_gets_the_tool_of_its_tool_call() {
    let turn = joined_turn("codex-ts", "codex", "mcp_tool").await;
    assert_eq!(turn.asked[0].tool, "search_notes");
    assert_eq!(turn.started()[0].tool, "search_notes");
}
