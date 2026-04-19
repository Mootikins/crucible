use super::super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

// ─── Setup-event translation (Task 1.3) ─────────────────────────────

#[test]
fn translate_session_initialized_produces_payload_msg() {
    use serde_json::json;
    let data = json!({
        "model": "glm-5",
        "mode": "plan",
        "agent_name": "claude",
        "kiln_path": "/k",
        "workspace_path": "/w",
    });
    let msgs = session_event_to_chat_msgs("session_initialized", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::SessionInitialized(p) => {
            assert_eq!(p.model, "glm-5");
            assert_eq!(p.mode, "plan");
            assert_eq!(p.agent_name.as_deref(), Some("claude"));
        }
        other => panic!("expected SessionInitialized, got {other:?}"),
    }
}

#[test]
fn translate_providers_listed_carries_providers() {
    use serde_json::json;
    let data = json!({
        "providers": [{
            "name": "OpenAI", "provider_type": "openai", "available": true,
            "default_model": null, "models": [], "endpoint": null,
            "reason": null, "is_local": false,
        }],
    });
    let msgs = session_event_to_chat_msgs("providers_listed", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ProvidersListed(providers) => {
            assert_eq!(providers.len(), 1);
            assert_eq!(providers[0].name, "OpenAI");
        }
        other => panic!("expected ProvidersListed, got {other:?}"),
    }
}

#[test]
fn translate_context_limit_resolved_parses_source() {
    use crucible_core::protocol::session_events::ContextLimitSource;
    use serde_json::json;
    let data = json!({ "limit": 128_000, "source": "provider_api" });
    let msgs = session_event_to_chat_msgs("context_limit_resolved", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ContextLimitResolved { limit, source } => {
            assert_eq!(*limit, 128_000);
            assert_eq!(*source, ContextLimitSource::ProviderApi);
        }
        other => panic!("expected ContextLimitResolved, got {other:?}"),
    }
}

#[test]
fn translate_workspace_indexed_carries_files() {
    use serde_json::json;
    let data = json!({ "files": ["src/lib.rs", "README.md"] });
    let msgs = session_event_to_chat_msgs("workspace_indexed", &data);
    match msgs.as_slice() {
        [ChatAppMsg::WorkspaceIndexed(files)] => assert_eq!(
            files,
            &vec!["src/lib.rs".to_string(), "README.md".to_string()]
        ),
        other => panic!("expected WorkspaceIndexed, got {other:?}"),
    }
}

#[test]
fn translate_kiln_notes_indexed_carries_notes() {
    use serde_json::json;
    let data = json!({ "notes": ["note:Daily.md"] });
    let msgs = session_event_to_chat_msgs("kiln_notes_indexed", &data);
    match msgs.as_slice() {
        [ChatAppMsg::KilnNotesIndexed(notes)] => {
            assert_eq!(notes, &vec!["note:Daily.md".to_string()])
        }
        other => panic!("expected KilnNotesIndexed, got {other:?}"),
    }
}

#[test]
fn translate_plugins_discovered_carries_entries() {
    use serde_json::json;
    let data = json!({
        "plugins": [
            { "name": "kiln-expert", "version": "0.1.0", "state": "loaded", "error": null }
        ]
    });
    let msgs = session_event_to_chat_msgs("plugins_discovered", &data);
    match msgs.as_slice() {
        [ChatAppMsg::PluginsDiscovered(entries)] => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "kiln-expert");
            assert_eq!(entries[0].state, "loaded");
        }
        other => panic!("expected PluginsDiscovered, got {other:?}"),
    }
}

#[test]
fn translate_mcp_servers_ready_maps_to_display_and_collapses_tools() {
    use serde_json::json;
    let data = json!({
        "servers": [
            {
                "name": "context7",
                "prefix": "c7_",
                "tools": ["query-docs", "resolve-library-id"],
                "connected": true,
            }
        ]
    });
    let msgs = session_event_to_chat_msgs("mcp_servers_ready", &data);
    match msgs.as_slice() {
        [ChatAppMsg::McpServersReady(servers)] => {
            assert_eq!(servers.len(), 1);
            assert_eq!(servers[0].name, "context7");
            // trailing `_` stripped to match legacy McpServerDisplay shape
            assert_eq!(servers[0].prefix, "c7");
            assert_eq!(servers[0].tool_count, 2);
            assert!(servers[0].connected);
        }
        other => panic!("expected McpServersReady, got {other:?}"),
    }
}

#[test]
fn translate_bad_payload_shape_returns_empty() {
    use serde_json::json;
    // Missing required fields — the type-strict deserializer fails and the
    // translator returns an empty vec rather than panicking.
    let msgs = session_event_to_chat_msgs("context_limit_resolved", &json!({}));
    assert!(msgs.is_empty());
}

#[test]
fn translate_unknown_event_returns_empty() {
    use serde_json::json;
    let msgs = session_event_to_chat_msgs("never_heard_of_it", &json!({}));
    assert!(msgs.is_empty());
}

#[test]
fn translate_context_limit_resolved_updates_atomic_through_stream() {
    use serde_json::json;
    let limit = Arc::new(AtomicUsize::new(0));
    let mut stream = SessionEventStream::new().with_context_limit(limit.clone());
    let msgs = stream.translate(
        "context_limit_resolved",
        &json!({ "limit": 4096, "source": "config" }),
    );
    assert_eq!(msgs.len(), 1);
    assert_eq!(limit.load(Ordering::Relaxed), 4096);
}

/// `ended { reason: "error: ..." }` must promote to `ChatAppMsg::Error`
/// through the unified consumer regardless of mode (live vs replay).
/// This is the Task 2.5 invariant: replay of an error-ending recording
/// surfaces the error identically to a live session that hit it.
#[tokio::test]
async fn consumer_promotes_ended_error_in_both_modes() {
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    for context_limit in [None, Some(Arc::new(AtomicUsize::new(0)))] {
        let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let session_id = "test-session-ended-error".to_string();
        let sid_clone = session_id.clone();
        let ctx_limit = context_limit.clone();

        let consumer = tokio::spawn(async move {
            session_event_consumer(sid_clone, event_rx, msg_tx, ctx_limit).await;
        });

        event_tx
            .send(crucible_daemon::SessionEvent {
                session_id: session_id.clone(),
                event_type: "ended".to_string(),
                data: json!({ "reason": "error: LLM timeout" }),
            })
            .unwrap();
        drop(event_tx);

        let msg = timeout(Duration::from_secs(1), msg_rx.recv())
            .await
            .expect("timely")
            .expect("some msg");
        match msg {
            ChatAppMsg::Error(s) => assert_eq!(s, "LLM timeout"),
            other => panic!("expected Error, got {:?}", other),
        }

        consumer.abort();
    }
}
