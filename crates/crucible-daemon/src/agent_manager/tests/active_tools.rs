//! The advertisement half of `cru.tools.set_active`, end to end.
//!
//! The dispatch half is covered in `reactor.rs` against an injected mock
//! agent. That mock replaces the handle the factory builds, so it cannot see
//! the other half at all: whether the manager's registry actually reaches the
//! `GenaiAgentHandle` that assembles the request. These tests therefore drive
//! a **real** handle — built by `build_agent_from_config` through
//! `send_message`, exactly as a turn does — at a mock provider endpoint, and
//! read the tool list out of the request that arrives.
//!
//! Three hops have to hold for that list to be narrowed:
//! `AgentManager::active_tools()` → `CreateAgentFromSessionConfigParams` →
//! `GenaiAgentHandle::with_active_tools`. Deleting any one of them leaves the
//! full tool list in the request.

use super::*;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Run one turn against `server` and return the tool names the provider
/// request advertised.
///
/// The response is a bare `[DONE]`: what the model says is irrelevant here,
/// the request is the artifact under test.
async fn advertised_tool_names(active: Option<Vec<&str>>) -> Vec<String> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: [DONE]\n\n"),
        )
        .mount(&server)
        .await;

    let mut h = ReactorTestHarness::new().await;
    // An endpoint the test owns, so the real genai client is exercised
    // without reaching a real provider.
    h.reconfigure(SessionAgent {
        endpoint: Some(server.uri()),
        ..test_agent()
    })
    .await;
    if let Some(patterns) = active {
        h.agent_manager.active_tools().set(
            &h.session_id,
            patterns.into_iter().map(String::from).collect(),
        );
    }

    h.send("hello").await;

    // Poll rather than sleep: the turn runs on its own task, and the only
    // signal that matters is the request landing.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        for req in server.received_requests().await.unwrap_or_default() {
            let Ok(body) = serde_json::from_slice::<serde_json::Value>(&req.body) else {
                continue;
            };
            let Some(tools) = body.get("tools").and_then(|t| t.as_array()) else {
                continue;
            };
            return tools
                .iter()
                .filter_map(|t| t.pointer("/function/name").and_then(|n| n.as_str()))
                .map(String::from)
                .collect();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no provider request carrying a tool list arrived"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Baseline: without a set the session advertises its whole tool surface.
/// Without this the narrowing assertion below could pass against a session
/// that never had the tools in the first place.
#[tokio::test]
async fn without_a_set_the_whole_tool_surface_is_advertised() {
    let names = advertised_tool_names(None).await;
    assert!(names.contains(&"read_file".to_string()), "{names:?}");
    assert!(names.contains(&"get_kiln_info".to_string()), "{names:?}");
    assert!(names.len() > 3, "expected the full surface, got {names:?}");
}

/// The registry the manager owns must reach the handle that builds the
/// request. Every hop between them is a place the set can be dropped, and
/// dropping it fails open — the model is offered every tool while the plugin
/// believes it narrowed the session.
#[tokio::test]
async fn an_active_set_narrows_what_the_provider_request_advertises() {
    let names = advertised_tool_names(Some(vec!["get_kiln_info"])).await;
    assert_eq!(
        names,
        vec!["get_kiln_info".to_string()],
        "the request must carry only what the active set names"
    );
}
