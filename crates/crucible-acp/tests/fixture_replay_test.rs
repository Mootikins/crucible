//! Integration test: drive a real `CrucibleAcpClient` against a recorded
//! fixture and assert the streaming round trip produces the expected data.
//!
//! The fixture was captured live from Claude Code 2.1.114 via:
//!
//! ```sh
//! CRUCIBLE_ACP_RECORD_DIR=/tmp/acp cru session create -a claude --permissions allow
//! cru session send <id> "say hello in exactly 3 words" --permissions allow
//! ```
//!
//! Sanitized by replacing `/home/moot` → `<HOME>`. See
//! `tests/fixtures/recorded/claude/basic-chat.jsonl`.

use std::path::{Path, PathBuf};

use agent_client_protocol::{
    ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, TextContent,
};
use crucible_acp::client::replay::{ReplayFixture, ReplayOutcome};
use crucible_acp::CrucibleAcpClient;

fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/recorded")
        .join(rel)
}

#[tokio::test]
async fn claude_basic_chat_replays_cleanly() {
    let path = fixture_path("claude/basic-chat.jsonl");
    let fixture = ReplayFixture::load(&path)
        .unwrap_or_else(|e| panic!("load fixture {}: {e}", path.display()));

    assert_eq!(fixture.header.agent, "claude");
    let original_records = fixture.records.len();

    let (writer, reader, driver) = fixture.into_transport();
    let driver_handle = tokio::spawn(driver);

    // Build a client wired to the replay transport. The agent_path is unused
    // when a transport is supplied.
    let config = crucible_acp::client::ClientConfig {
        agent_path: PathBuf::from("/dev/null"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5_000),
        max_retries: None,
    };
    let mut client = CrucibleAcpClient::with_transport(config, writer, reader);

    // Drive the protocol.
    let init = client
        .initialize(InitializeRequest::new(1u16.into()))
        .await
        .expect("initialize");
    assert_eq!(init.protocol_version, 1u16.into());

    let session = client
        .create_new_session(NewSessionRequest::new(PathBuf::from("/<HOME>/.crucible")))
        .await
        .expect("create session");
    assert!(!session.session_id.0.is_empty(), "session id is non-empty");

    let prompt = PromptRequest::new(
        session.session_id.clone(),
        vec![ContentBlock::Text(TextContent::new(
            "say hello in exactly 3 words",
        ))],
    );
    let (text, tools, prompt_response) = client
        .send_prompt_with_streaming(prompt)
        .await
        .expect("send prompt");

    // The fixture's response said "Hello to you!" — assert we reassembled it.
    assert!(
        text.contains("Hello to you!"),
        "expected reassembled text to contain greeting; got {text:?}"
    );
    assert!(tools.is_empty(), "no tool calls expected in greeting flow");

    // Crucially: the fixture carried usage data on the final response.
    // The PromptResponse in agent-client-protocol exposes it via .meta — we
    // just assert the response parsed and the stop reason matches.
    let stop = format!("{:?}", prompt_response.stop_reason);
    assert!(
        stop.to_lowercase().contains("end") || stop.to_lowercase().contains("turn"),
        "expected end-of-turn stop reason; got {stop}"
    );

    drop(client);
    let outcome: ReplayOutcome = driver_handle.await.expect("driver panicked");
    assert!(
        outcome.is_clean(),
        "fixture replay diverged: {:?}",
        outcome.divergences
    );
    assert_eq!(
        outcome.frames_consumed, original_records,
        "all fixture frames should be consumed during replay"
    );
}
