//! An ACP agent's call to a Crucible MCP tool answers to the session's
//! containment.
//!
//! The daemon gives each ACP agent an in-process MCP server and offers its
//! HTTP URL in `session/new`. `mcp_host.rs` tests that server directly. These
//! tests make the AGENT PROCESS call the tool: the mock agent reads the URL it
//! received, calls `read_note` over HTTP (`CRU_MOCK_MCP_CALL`), and writes the
//! reply to a capture file. The session comes from `AgentManager`, so the
//! tool set is the one the daemon builds for a real kiln session.
//!
//! The mock advertises `mcpCapabilities.http`, so the daemon offers the HTTP
//! URL and not the stdio command.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::session::SessionType;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::AgentManager;
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::{
    acp_manager_params, completed_turn, mock_profile, profile_session_agent, MOCK_PROFILE,
};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// What the mock streams after the tool call, so the turn has a text answer.
const ANSWER: &str = "tool call done";

/// Run one turn in a session over the kiln `kiln`, with `other` registered as
/// a second kiln that the session does not attach. The agent calls
/// `read_note` with `path`. Return the reply that the agent process received.
async fn agent_reads(temp: &TempDir, kiln: &Path, other: &Path, path: &str) -> String {
    let capture = temp.path().join("mcp-capture.json");
    let session_manager = temp_session_manager_with_kilns(&[("kiln", kiln), ("other-kiln", other)]);
    let (event_tx, _events) = broadcast::channel::<SessionEventMessage>(256);
    let profile = mock_profile(BTreeMap::from([
        (
            "CRU_MOCK_MCP_CALL".to_string(),
            format!("read_note:{}", serde_json::json!({ "path": path })),
        ),
        (
            "CRU_MOCK_MCP_CAPTURE".to_string(),
            capture.to_string_lossy().into_owned(),
        ),
        ("CRU_MOCK_STREAM_CHUNKS".to_string(), ANSWER.to_string()),
    ]));
    let agent_manager = Arc::new(AgentManager::new(acp_manager_params(
        session_manager.clone(),
        BTreeMap::from([(MOCK_PROFILE.to_string(), profile)]),
        &event_tx,
    )));
    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");
    agent_manager
        .configure_agent(&session.id, profile_session_agent(MOCK_PROFILE))
        .await
        .expect("configure the agent");

    let (_id, done) = agent_manager
        .send_message_notified(&session.id, "read it".to_string(), &event_tx, true, None)
        .await
        .expect("the turn is accepted");
    // The mock writes the capture before it answers the prompt, so a
    // completed turn means the capture is complete.
    let outcome = completed_turn(done, TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "the agent's answer");

    std::fs::read_to_string(&capture).unwrap_or_else(|e| {
        panic!(
            "the agent wrote no MCP capture at {} ({e})",
            capture.display()
        )
    })
}

/// Two kiln directories: the session's kiln with one note, and a second kiln
/// with a secret.
fn kilns(temp: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let kiln = temp.path().join("kiln");
    let other = temp.path().join("other");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    std::fs::create_dir_all(&other).expect("other kiln dir");
    std::fs::write(kiln.join("inside.md"), "KILN-GROUNDED-CONTENT").expect("note");
    std::fs::write(other.join("secret.md"), "OTHER-KILN-SECRET").expect("secret");
    (kiln, other)
}

/// The agent reads a note in the session's kiln and gets its text.
#[tokio::test]
async fn an_acp_agent_reads_a_note_in_the_session_kiln_over_mcp() {
    let temp = TempDir::new().expect("temp dir");
    let (kiln, other) = kilns(&temp);

    let reply = agent_reads(&temp, &kiln, &other, "inside.md").await;
    assert!(
        reply.contains("KILN-GROUNDED-CONTENT"),
        "the agent must receive the note text: {reply}"
    );
}

/// The agent reads through a symlink into a kiln the session does not attach.
/// The reply is a refusal and does not hold the secret.
///
/// The kiln boundary of the note tools refuses this read. The session root
/// set does not decide it: the read is refused with `RootSet::Ambient` too.
/// The registry does not let a kiln enclose the sessions root, so no read of
/// a registered kiln depends on the root set alone.
#[tokio::test]
async fn an_acp_agent_cannot_read_a_kiln_the_session_does_not_attach() {
    let temp = TempDir::new().expect("temp dir");
    let (kiln, other) = kilns(&temp);
    std::os::unix::fs::symlink(other.join("secret.md"), kiln.join("elsewhere.md"))
        .expect("symlink");

    let reply = agent_reads(&temp, &kiln, &other, "elsewhere.md").await;
    assert!(
        !reply.contains("OTHER-KILN-SECRET"),
        "the agent read a kiln that the session does not attach: {reply}"
    );
    let reply: serde_json::Value = serde_json::from_str(&reply)
        .unwrap_or_else(|e| panic!("the capture must be a JSON-RPC reply ({e}): {reply}"));
    assert!(
        reply.get("error").is_some() || reply["result"]["isError"] == true,
        "the read must be refused: {reply}"
    );
}
