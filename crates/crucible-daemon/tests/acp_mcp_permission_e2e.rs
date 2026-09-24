//! An ACP agent's call to a Crucible MCP tool passes the one tool policy.
//!
//! The agent runs its own tools, and it calls Crucible's tools through the
//! in-process MCP server. That server runs the call in the daemon, so the
//! daemon decides it with `decide_permission`, as it decides a call of its
//! own agent: the card `tool_policy`, the `[permissions]` rules, the saved
//! patterns, the hooks and the prompt. The agent does not have to ask.
//!
//! The mock agent process makes the call over HTTP (`Step::McpCall`) and
//! logs the reply as `mcp/result`.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
use crucible_core::config::components::permissions::PermissionConfig;
use crucible_core::interaction::PermResponse;
use tempfile::TempDir;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{logged, MockScript, Step};
use mock_agent_bin::{mock_session_with, MockSession};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);
const ANSWER: &str = "tool call done";

/// A kiln with one note.
fn kiln(temp: &TempDir) -> PathBuf {
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    std::fs::write(kiln.join("inside.md"), "KILN-GROUNDED-CONTENT").expect("note");
    kiln
}

fn script(log: &Path, turn: Vec<Step>) -> MockScript {
    MockScript {
        turn: [turn, vec![Step::Text(ANSWER.to_string())]].concat(),
        log: Some(log.to_path_buf()),
        ..MockScript::default()
    }
}

fn read_note() -> Step {
    Step::McpCall {
        tool: "read_note".to_string(),
        args: serde_json::json!({ "path": "inside.md" }),
    }
}

/// Run one turn and return the MCP reply that the agent logged.
async fn mcp_reply(session: &MockSession, log: &Path) -> String {
    let outcome = session.turn("read it", TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "the agent's answer");
    logged(log, "mcp/result")
        .first()
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("the agent logged no MCP reply at {}", log.display()))
        .to_string()
}

fn assert_refused(reply: &str, why: &str) {
    assert!(
        !reply.contains("KILN-GROUNDED-CONTENT"),
        "the refused call ran: {reply}"
    );
    let json: serde_json::Value = serde_json::from_str(reply)
        .unwrap_or_else(|e| panic!("the capture must be a JSON-RPC reply ({e}): {reply}"));
    assert_eq!(json["result"]["isError"], true, "a refusal: {reply}");
    assert!(reply.contains(why), "the refusal says why ({why}): {reply}");
}

/// A card `deny` refuses the agent's Crucible MCP call, and the agent
/// reads why. `read_note` is read-only, so nothing else would stop it.
#[tokio::test]
async fn a_card_deny_refuses_an_acp_agents_crucible_mcp_call() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = kiln(&temp);
    let log = temp.path().join("mock-agent.log");
    let card = ToolPolicyMap::from([("read_note".to_string(), ToolPolicy::Deny)]);
    let session = mock_session_with(
        &[("kiln", &kiln)],
        None,
        script(&log, vec![read_note()]),
        Some(card),
        None,
    )
    .await;

    assert_refused(&mcp_reply(&session, &log).await, "card tool policy");
}

/// An operator `deny` rule refuses it too.
#[tokio::test]
async fn an_operator_rule_refuses_an_acp_agents_crucible_mcp_call() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = kiln(&temp);
    let log = temp.path().join("mock-agent.log");
    let rules = PermissionConfig {
        deny: vec!["read_note:*".to_string()],
        ..Default::default()
    };
    let session = mock_session_with(
        &[("kiln", &kiln)],
        None,
        script(&log, vec![read_note()]),
        None,
        Some(rules),
    )
    .await;

    assert_refused(
        &mcp_reply(&session, &log).await,
        "denied by permissions config",
    );
}

/// The agent asks about its call to Crucible's MCP server, and then makes
/// it. The MCP server decides the call, so the user sees one prompt, not
/// one for the question and one for the call. The note is written.
#[tokio::test]
async fn an_asked_crucible_mcp_call_prompts_the_user_once() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = kiln(&temp);
    let log = temp.path().join("mock-agent.log");
    let ask = Step::Permission(serde_json::json!({
        "toolCall": {
            "toolCallId": "toolu_01",
            "title": "mcp__crucible__create_note",
            "kind": "other",
            "rawInput": { "path": "new.md", "content": "WRITTEN" },
        },
        "options": [
            { "optionId": "allow_once", "name": "Allow", "kind": "allow_once" },
            { "optionId": "reject_once", "name": "Reject", "kind": "reject_once" },
        ],
    }));
    let write = Step::McpCall {
        tool: "create_note".to_string(),
        args: serde_json::json!({ "path": "new.md", "content": "WRITTEN" }),
    };
    let mut session = mock_session_with(
        &[("kiln", &kiln)],
        None,
        script(&log, vec![ask, write]),
        None,
        None,
    )
    .await;

    // The user answers each prompt with allow, and counts them.
    let manager = session.agent_manager.clone();
    let session_id = session.session_id.to_string();
    let mut events = std::mem::replace(&mut session.events, session.event_tx.subscribe());
    let user = tokio::spawn(async move {
        let mut prompts = 0;
        while let Ok(event) = events.recv().await {
            if event.event == "interaction_requested" {
                prompts += 1;
                let id = event.data["request_id"].as_str().unwrap().to_string();
                let _ = manager.respond_to_permission(&session_id, &id, PermResponse::allow());
            }
            if event.event == "turn_finished" {
                break;
            }
        }
        prompts
    });

    let reply = mcp_reply(&session, &log).await;
    let prompts = tokio::time::timeout(Duration::from_secs(10), user)
        .await
        .expect("the turn ends")
        .expect("join");
    assert_eq!(prompts, 1, "one call, one prompt; the reply was {reply}");
    assert!(
        kiln.join("new.md").exists(),
        "the allowed write runs: {reply}"
    );
}
