//! Integration tests for Rune handlers emitting events through the Reactor.
//!
//! These tests verify the full flow:
//! 1. Rune handler is discovered and registered
//! 2. Reactor dispatches event to handler
//! 3. Handler emits additional events via ctx.emit()
//! 4. Emitted events are collected by the Reactor

use crucible_core::events::{Handler, HandlerContext, Reactor, SessionEvent};
use crucible_rune::core_handler::{RuneHandler, RuneHandlerMeta};
use crucible_rune::RuneExecutor;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_executor() -> Arc<RuneExecutor> {
    Arc::new(RuneExecutor::new().expect("Failed to create executor"))
}

#[tokio::test]
async fn test_handler_emits_single_event() {
    let temp = TempDir::new().unwrap();
    let script_path = temp.path().join("emit_handler.rn");

    fs::write(
        &script_path,
        r#"
pub fn handle(ctx, event) {
    #{
        emit: [#{ type: "session_paused", session_id: "test-session" }]
    }
}
"#,
    )
    .unwrap();

    let executor = create_test_executor();
    let meta = RuneHandlerMeta::new(&script_path, "handle").with_event_pattern("*");
    let handler = RuneHandler::new(meta, executor).unwrap();

    let mut ctx = HandlerContext::new();
    let event = SessionEvent::Custom {
        name: "test_trigger".into(),
        payload: serde_json::json!({}),
    };

    let result = handler.handle(&mut ctx, event).await;

    assert!(result.is_continue());
    let emitted = ctx.take_emitted();
    assert_eq!(emitted.len(), 1);
    assert!(matches!(emitted[0], SessionEvent::SessionPaused { .. }));
}

#[tokio::test]
async fn test_handler_emits_multiple_events() {
    let temp = TempDir::new().unwrap();
    let script_path = temp.path().join("multi_emit.rn");

    fs::write(
        &script_path,
        r#"
pub fn handle(ctx, event) {
    #{
        emit: [
            #{ type: "session_paused", session_id: "s1" },
            #{ type: "session_resumed", session_id: "s2" }
        ]
    }
}
"#,
    )
    .unwrap();

    let executor = create_test_executor();
    let meta = RuneHandlerMeta::new(&script_path, "handle").with_event_pattern("*");
    let handler = RuneHandler::new(meta, executor).unwrap();

    let mut ctx = HandlerContext::new();
    let event = SessionEvent::Custom {
        name: "trigger".into(),
        payload: serde_json::json!({}),
    };

    let result = handler.handle(&mut ctx, event).await;

    assert!(result.is_continue());
    let emitted = ctx.take_emitted();
    assert_eq!(emitted.len(), 2);
    assert!(matches!(emitted[0], SessionEvent::SessionPaused { .. }));
    assert!(matches!(emitted[1], SessionEvent::SessionResumed { .. }));
}

#[tokio::test]
async fn test_handler_emits_and_modifies_event() {
    let temp = TempDir::new().unwrap();
    let script_path = temp.path().join("emit_and_modify.rn");

    fs::write(
        &script_path,
        r#"
pub fn handle(ctx, event) {
    #{
        emit: [
            #{ type: "session_paused", session_id: "side-effect" }
        ],
        event: #{ type: "custom", name: "modified", payload: #{} }
    }
}
"#,
    )
    .unwrap();

    let executor = create_test_executor();
    let meta = RuneHandlerMeta::new(&script_path, "handle").with_event_pattern("*");
    let handler = RuneHandler::new(meta, executor).unwrap();

    let mut ctx = HandlerContext::new();
    let event = SessionEvent::Custom {
        name: "original".into(),
        payload: serde_json::json!({}),
    };

    let result = handler.handle(&mut ctx, event).await;

    assert!(result.is_continue());
    let modified = result.event().unwrap();
    if let SessionEvent::Custom { name, .. } = modified {
        assert_eq!(name, "modified");
    } else {
        panic!("Expected Custom event");
    }

    let emitted = ctx.take_emitted();
    assert_eq!(emitted.len(), 1);
}

#[tokio::test]
async fn test_handler_registers_with_reactor() {
    let temp = TempDir::new().unwrap();
    let script_path = temp.path().join("reactor_emit.rn");

    fs::write(
        &script_path,
        r#"
pub fn handle(ctx, event) {
    event
}
"#,
    )
    .unwrap();

    let executor = create_test_executor();
    let meta = RuneHandlerMeta::new(&script_path, "handle").with_event_pattern("*");
    let handler = RuneHandler::new(meta, executor).unwrap();

    let mut reactor = Reactor::new();
    let result = reactor.register(Box::new(handler));
    assert!(result.is_ok());
    assert_eq!(reactor.handler_count(), 1);
}

#[tokio::test]
async fn test_handler_with_attribute_stripped_compiles() {
    let temp = TempDir::new().unwrap();
    let script_path = temp.path().join("attributed.rn");

    fs::write(
        &script_path,
        r#"
#[handler(event = "tool:after", pattern = "*", priority = 10)]
pub fn attributed_handler(ctx, event) {
    ctx.emit(#{ type: "session_paused", session_id: "test" });
    event
}
"#,
    )
    .unwrap();

    let executor = create_test_executor();
    let meta = RuneHandlerMeta::new(&script_path, "attributed_handler")
        .with_event_pattern("tool:after")
        .with_priority(10);

    let handler = RuneHandler::new(meta, executor);
    assert!(handler.is_ok());
}
