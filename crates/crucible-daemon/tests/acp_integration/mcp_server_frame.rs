//! What the client actually puts in `session/new`'s `mcpServers`.
//!
//! `acp_transport_negotiation.rs` asserts the *flags* the client stored from
//! `initialize` — `agent_supports_http_mcp()` — and that a session opened. It
//! never reads the frame. Invert the http/stdio branch in
//! `connection.rs::handshake` and every one of those
//! tests still passes, because the mock accepts whatever `mcpServers` it is
//! handed and records nothing about them.
//!
//! The choice is not cosmetic. An agent given a stdio entry it cannot start
//! reports the failure mid-turn (codex-acp announces it as a `session/update`
//! before answering), and an agent given an HTTP url it cannot reach loses
//! every Crucible tool for the session. So the frame itself is the assertion
//! here: which transport, and for stdio the exact command and argv, since
//! `build_stdio_mcp_server` derives the path from `current_exe` and nothing
//! else checks what it produced.

use crate::scripted_agent::{client_with_custom_transport, read_frame, write_json_line};
use serde_json::json;

/// Run the handshake against a scripted agent and hand back the `mcpServers`
/// array the client sent on `session/new`.
///
/// `http_mcp` is what the agent advertises in `initialize`; `mcp_url` is what
/// the daemon would pass when it has an in-process MCP host running.
async fn mcp_servers_sent(http_mcp: bool, mcp_url: Option<&str>) -> Vec<serde_json::Value> {
    let (mut client, mut agent_read, mut agent_write) =
        client_with_custom_transport(Some(5_000)).await;

    let agent = tokio::spawn(async move {
        let init = read_frame(&mut agent_read).await;
        assert_eq!(init["method"], "initialize");
        write_json_line(
            &mut agent_write,
            json!({
                "jsonrpc": "2.0",
                "id": init["id"],
                "result": {
                    "protocolVersion": 1,
                    "agentCapabilities": {
                        "loadSession": true,
                        "mcpCapabilities": {"http": http_mcp, "sse": false}
                    },
                    "authMethods": []
                }
            }),
        )
        .await;

        let new_session = read_frame(&mut agent_read).await;
        assert_eq!(new_session["method"], "session/new");
        write_json_line(
            &mut agent_write,
            json!({
                "jsonrpc": "2.0",
                "id": new_session["id"],
                "result": {"sessionId": "sess-mcp-frame"}
            }),
        )
        .await;

        (new_session, agent_write)
    });

    client
        .handshake(mcp_url, None)
        .await
        .expect("the scripted agent completes the handshake");

    let (new_session, _writer) = agent.await.expect("the scripted agent finished");

    new_session["params"]["mcpServers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn an_http_capable_agent_is_sent_the_http_mcp_url() {
    let servers = mcp_servers_sent(true, Some("http://127.0.0.1:9999/mcp")).await;

    assert_eq!(servers.len(), 1, "exactly one MCP server is offered");
    let server = &servers[0];

    assert_eq!(
        server["type"], "http",
        "an agent advertising mcpCapabilities.http must get the HTTP transport; got {server}"
    );
    assert_eq!(
        server["url"], "http://127.0.0.1:9999/mcp",
        "the HTTP entry must carry the host's own url"
    );
    assert_eq!(server["name"], "crucible");
}

#[tokio::test]
async fn an_agent_without_http_support_is_sent_the_stdio_mcp_server() {
    let servers = mcp_servers_sent(false, Some("http://127.0.0.1:9999/mcp")).await;

    assert_eq!(servers.len(), 1, "exactly one MCP server is offered");
    let server = &servers[0];

    assert_ne!(
        server["type"], "http",
        "an agent that did not advertise HTTP MCP must not be handed a url it cannot use; got {server}"
    );

    let command = server["command"]
        .as_str()
        .unwrap_or_else(|| panic!("the stdio entry must carry a command; got {server}"));
    assert!(
        command.ends_with("cru"),
        "the stdio MCP server is Crucible's own binary; got {command:?}"
    );

    let args: Vec<&str> = server["args"]
        .as_array()
        .unwrap_or_else(|| panic!("the stdio entry must carry args; got {server}"))
        .iter()
        .map(|a| a.as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        args,
        ["mcp", "--stdio", "--standalone"],
        "the stdio MCP server runs `cru mcp --stdio --standalone`"
    );
    assert_eq!(server["name"], "crucible");
}

#[tokio::test]
async fn an_http_capable_agent_with_no_host_still_gets_stdio() {
    // The daemon passes `None` when no in-process MCP host is running — the
    // agent's capability cannot conjure a url that does not exist.
    let servers = mcp_servers_sent(true, None).await;

    assert_eq!(servers.len(), 1, "exactly one MCP server is offered");
    assert_ne!(
        servers[0]["type"], "http",
        "with no host url there is nothing to point HTTP at; got {}",
        servers[0]
    );
    assert!(
        servers[0]["command"].is_string(),
        "the fallback is the stdio entry; got {}",
        servers[0]
    );
}
