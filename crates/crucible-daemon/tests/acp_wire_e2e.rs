//! The ACP wire between the daemon and a spawned agent process.
//!
//! Each test runs the `mock-acp-agent` binary through `AgentManager`, so the
//! frames cross a real process pipe and a real `AcpAgentHandle`.

use std::time::Duration;

use tempfile::TempDir;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{MockScript, Step};
use mock_agent_bin::mock_session;

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// Run one turn of `script` and return the text of the turn.
async fn turn_text(script: MockScript) -> String {
    let kiln = TempDir::new().expect("kiln dir");
    let session = mock_session(&[("kiln", kiln.path())], script).await;
    session.turn("go", TURN_TIMEOUT).await.final_text
}

/// Two "\n" chunks in a row are two lines, not a resend of the answer.
/// The ACP client keeps both (c122365e4), and the daemon must keep both too.
#[tokio::test]
async fn a_second_newline_chunk_is_not_a_resend() {
    let script = MockScript {
        turn: ["\n", "\n", "Hi"].map(|t| Step::Text(t.to_string())).into(),
        ..MockScript::default()
    };
    assert_eq!(turn_text(script).await, "\n\nHi");
}

/// A final chunk that repeats the whole answer is a resend, and the daemon
/// drops it.
#[tokio::test]
async fn a_chunk_that_repeats_the_answer_is_a_resend() {
    let script = MockScript {
        turn: ["Hello", " world", "Hello world"]
            .map(|t| Step::Text(t.to_string()))
            .into(),
        ..MockScript::default()
    };
    assert_eq!(turn_text(script).await, "Hello world");
}
