//! An ACP agent's call to a Crucible MCP tool answers to the session's
//! containment.
//!
//! The daemon gives each ACP agent an in-process MCP server and offers its
//! HTTP URL in `session/new`. `mcp_host.rs` tests that server directly. These
//! tests make the AGENT PROCESS call the tool: the mock agent reads the URL it
//! received, calls `read_note` over HTTP (`Step::McpCall`), and logs the
//! reply as `mcp/result`. The session comes from `AgentManager`, so the
//! tool set is the one the daemon builds for a real kiln session.
//!
//! The mock advertises `mcpCapabilities.http`, so the daemon offers the HTTP
//! URL and not the stdio command.

#![cfg(unix)]

use std::path::Path;
use std::time::Duration;

use tempfile::TempDir;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, MockScript, Step};
use mock_agent_bin::mock_session;

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// What the mock streams after the tool call, so the turn has a text answer.
const ANSWER: &str = "tool call done";

/// Run one turn in a session over the kiln `kiln`, with `other` registered as
/// a second kiln that the session does not attach. The agent calls
/// `read_note` with `path`. Return the reply that the agent process received.
async fn agent_reads(temp: &TempDir, kiln: &Path, other: &Path, path: &str) -> String {
    let args = serde_json::json!({ "path": path });
    agent_calls(temp, kiln, other, "read_note", args).await
}

/// As `agent_reads`, but the agent calls the MCP tool `tool` with `args`.
async fn agent_calls(
    temp: &TempDir,
    kiln: &Path,
    other: &Path,
    tool: &str,
    args: serde_json::Value,
) -> String {
    let log = temp.path().join("mock-agent.log");
    let script = MockScript {
        turn: vec![
            Step::McpCall {
                tool: tool.to_string(),
                args,
            },
            Step::Text(ANSWER.to_string()),
        ],
        log: Some(log.clone()),
        ..MockScript::default()
    };
    let mut session = mock_session(&[("kiln", kiln), ("other-kiln", other)], None, script).await;

    // The permission gate asks about a call that can write. The user allows
    // it, so what refuses the call is the containment under test.
    let manager = session.agent_manager.clone();
    let session_id = session.session_id.to_string();
    let mut events = std::mem::replace(&mut session.events, session.event_tx.subscribe());
    let user = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if event.event == "interaction_requested" {
                let id = event.data["request_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let _ = manager.respond_to_permission(
                    &session_id,
                    &id,
                    crucible_core::interaction::PermResponse::allow(),
                );
            }
        }
    });

    // The mock logs the MCP reply before it answers the prompt, so a
    // completed turn means the log holds the reply.
    let outcome = session.turn("read it", TURN_TIMEOUT).await;
    user.abort();
    assert_eq!(outcome.final_text.trim(), ANSWER, "the agent's answer");

    let results = logged(&log, "mcp/result");
    let reply = results
        .first()
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("the agent logged no MCP reply at {}", log.display()));
    reply.to_string()
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

/// The agent writes a note into a tree that the daemon loads plugins from.
/// The reply is a refusal and the file does not exist.
///
/// The session root set protects the trees the daemon executes. The name
/// check that also applies to `RootSet::Ambient` does not know these trees,
/// so this refusal depends on the session root set.
#[tokio::test]
async fn an_acp_agent_cannot_write_into_a_plugin_tree_inside_the_kiln() {
    let temp = TempDir::new().expect("temp dir");
    let (kiln, other) = kilns(&temp);
    // The tree exists, as a real plugin tree does. Without it, the write
    // fails because the directory is absent, and a gate break looks like
    // a refusal.
    let plugins = kiln.join("plugins");
    std::fs::create_dir_all(&plugins).expect("plugin tree");
    // The daemon reads this variable when it builds the session root set.
    // nextest runs each test in its own process.
    let _plugin_path = crucible_core::test_support::EnvVarGuard::set(
        "CRUCIBLE_PLUGIN_PATH",
        plugins.to_string_lossy().into_owned(),
    );

    let args = serde_json::json!({ "path": "plugins/evil.md", "content": "PLANTED" });
    let reply = agent_calls(&temp, &kiln, &other, "create_note", args).await;
    assert!(
        !plugins.join("evil.md").exists(),
        "the agent wrote into a tree that the daemon executes: {reply}"
    );
    let reply: serde_json::Value = serde_json::from_str(&reply)
        .unwrap_or_else(|e| panic!("the capture must be a JSON-RPC reply ({e}): {reply}"));
    assert!(
        reply.to_string().contains("write-protected"),
        "the write must be refused as a write to a protected tree: {reply}"
    );
}
