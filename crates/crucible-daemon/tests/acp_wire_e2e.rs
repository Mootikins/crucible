//! The ACP wire between the daemon and a spawned agent process.
//!
//! Each test runs the `mock-acp-agent` binary through `AgentManager`, so the
//! frames cross a real process pipe and a real `AcpAgentHandle`.

use std::time::Duration;

use serde_json::json;
use tempfile::TempDir;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, MockScript, Step};
use mock_agent_bin::{mock_session, MockSession};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// A session of `script` with the dir as its kiln and its workspace. The
/// agent logs its frames to `agent.log` in the dir.
async fn session(mut script: MockScript) -> (TempDir, MockSession) {
    let dir = TempDir::new().expect("temp dir");
    script.log = Some(dir.path().join("agent.log"));
    let session = mock_session(&[("kiln", dir.path())], Some(dir.path()), script).await;
    (dir, session)
}

/// Run one turn of `script` and return the text of the turn.
async fn turn_text(script: MockScript) -> String {
    let (_dir, session) = session(script).await;
    session.turn("go", TURN_TIMEOUT).await.final_text
}

fn texts<const N: usize>(texts: [&str; N]) -> Vec<Step> {
    texts.map(|t| Step::Text(t.to_string())).into()
}

/// Two "\n" chunks in a row are two lines, not a resend of the answer.
/// The ACP client keeps both (c122365e4), and the daemon must keep both too.
#[tokio::test]
async fn a_second_newline_chunk_is_not_a_resend() {
    let script = MockScript {
        turn: texts(["\n", "\n", "Hi"]),
        ..MockScript::default()
    };
    assert_eq!(turn_text(script).await, "\n\nHi");
}

/// A final chunk that repeats the whole answer is a resend, and the daemon
/// drops it.
#[tokio::test]
async fn a_chunk_that_repeats_the_answer_is_a_resend() {
    let script = MockScript {
        turn: texts(["Hello", " world", "Hello world"]),
        ..MockScript::default()
    };
    assert_eq!(turn_text(script).await, "Hello world");
}

/// An agent that runs under npx can print a line that is not JSON. The line
/// does not end the session: this turn and the next one complete.
#[tokio::test]
async fn a_line_that_is_not_json_does_not_break_the_session() {
    let script = MockScript {
        turn: vec![
            Step::Raw("npm warn this line is not JSON".to_string()),
            Step::Text("done".to_string()),
        ],
        ..MockScript::default()
    };
    let (_dir, session) = session(script).await;
    for _ in 0..2 {
        assert_eq!(session.turn("go", TURN_TIMEOUT).await.final_text, "done");
    }
}

/// codex-acp sends a `session/update` before it answers `session/new`. The
/// session opens, and the update is not part of the first turn.
#[tokio::test]
async fn an_update_before_the_handshake_answers_is_not_a_turn_update() {
    let script = MockScript {
        handshake_update: Some(json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "EARLY" },
        })),
        turn: texts(["done"]),
        ..MockScript::default()
    };
    assert_eq!(turn_text(script).await, "done");
}

/// `session/new` gives the agent the workspace of the session as `cwd`.
#[tokio::test]
async fn session_new_carries_the_workspace_as_cwd() {
    let script = MockScript {
        turn: texts(["done"]),
        ..MockScript::default()
    };
    let (dir, session) = session(script).await;
    session.turn("go", TURN_TIMEOUT).await;
    let workspace = dir.path().canonicalize().expect("canonical workspace");
    let sessions = logged(&dir.path().join("agent.log"), "session/new");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["cwd"], json!(workspace));
}
