//! Integration tests for the handler system

use crucible_rune::{
    DiscoveryPaths, Event, EventBus, EventType, Handler, HandlerManager, HandlerRegistry,
};
use serde_json::json;
use std::fs;
use tempfile::TempDir;

/// Test that Rune handlers can modify event payloads
#[tokio::test(flavor = "multi_thread")]
async fn test_rune_handler_modifies_payload() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    // Write a handler that adds a "processed" field to the payload
    let script = r#"
/// Adds processed flag to events
#[handler(event = "tool:after", pattern = "*")]
pub fn add_processed(ctx, event) {
    // Add processed flag to payload
    let payload = event.payload;
    payload.processed = true;
    event.payload = payload;
    event
}
"#;
    fs::write(handlers_dir.join("processor.rn"), script).unwrap();

    // Set up registry and discover
    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let mut registry = HandlerRegistry::with_paths(paths).unwrap();
    let count = registry.discover().unwrap();
    assert_eq!(count, 1);

    // Register on event bus
    let mut bus = EventBus::new();
    registry.register_all(&mut bus);

    // Emit event
    let event = Event::tool_after("test_tool", json!({"value": 42}));
    let (result, _ctx, errors) = bus.emit(event);

    // Verify no errors
    assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);

    // Verify payload was modified
    assert_eq!(result.payload["value"], json!(42));
    assert_eq!(result.payload["processed"], json!(true));
}

/// Test that handlers respect pattern matching
#[tokio::test(flavor = "multi_thread")]
async fn test_rune_handler_pattern_matching() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    // Write a handler that only matches "just_*" tools
    let script = r#"
/// Only processes just_* tools
#[handler(event = "tool:after", pattern = "just_*")]
pub fn just_only(ctx, event) {
    let payload = event.payload;
    payload.just_processed = true;
    event.payload = payload;
    event
}
"#;
    fs::write(handlers_dir.join("just_handler.rn"), script).unwrap();

    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let mut registry = HandlerRegistry::with_paths(paths).unwrap();
    registry.discover().unwrap();

    let mut bus = EventBus::new();
    registry.register_all(&mut bus);

    // Test matching event
    let just_event = Event::tool_after("just_test", json!({}));
    let (result, _, _) = bus.emit(just_event);
    assert_eq!(
        result.payload["just_processed"],
        json!(true),
        "just_* should be processed"
    );

    // Test non-matching event
    let rune_event = Event::tool_after("rune_tool", json!({}));
    let (result, _, _) = bus.emit(rune_event);
    assert!(
        result.payload.get("just_processed").is_none(),
        "rune_* should not be processed"
    );
}

/// Test handler priority ordering
#[tokio::test(flavor = "multi_thread")]
async fn test_rune_handler_priority_ordering() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    // Write multiple handlers with different priorities
    let script = r#"
/// First handler (priority 10 = runs first)
#[handler(event = "tool:after", pattern = "*", priority = 10)]
pub fn first_handler(ctx, event) {
    let payload = event.payload;
    let current = if payload.contains_key("order") { payload.order } else { "" };
    payload.order = current + "1";
    event.payload = payload;
    event
}

/// Second handler (priority 50)
#[handler(event = "tool:after", pattern = "*", priority = 50)]
pub fn second_handler(ctx, event) {
    let payload = event.payload;
    let current = if payload.contains_key("order") { payload.order } else { "" };
    payload.order = current + "2";
    event.payload = payload;
    event
}

/// Third handler (priority 100 = default)
#[handler(event = "tool:after", pattern = "*", priority = 100)]
pub fn third_handler(ctx, event) {
    let payload = event.payload;
    let current = if payload.contains_key("order") { payload.order } else { "" };
    payload.order = current + "3";
    event.payload = payload;
    event
}
"#;
    fs::write(handlers_dir.join("priority_handlers.rn"), script).unwrap();

    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let mut registry = HandlerRegistry::with_paths(paths).unwrap();
    registry.discover().unwrap();

    let mut bus = EventBus::new();
    registry.register_all(&mut bus);

    let event = Event::tool_after("test", json!({}));
    let (result, _, errors) = bus.emit(event);

    assert!(errors.is_empty(), "Expected no errors: {:?}", errors);
    assert_eq!(
        result.payload["order"],
        json!("123"),
        "Hooks should run in priority order"
    );
}

/// Test that handler errors don't break the pipeline (fail-open)
#[tokio::test(flavor = "multi_thread")]
async fn test_rune_handler_fail_open() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    // Write a handler that throws an error
    let script = r#"
/// This hook will fail
#[handler(event = "tool:after", pattern = "*", priority = 10)]
pub fn failing_handler(ctx, event) {
    // This will cause a runtime error - panic
    panic!("intentional failure")
}

/// This handler should still run
#[handler(event = "tool:after", pattern = "*", priority = 100)]
pub fn succeeding_handler(ctx, event) {
    let payload = event.payload;
    payload.success = true;
    event.payload = payload;
    event
}
"#;
    fs::write(handlers_dir.join("fail_handlers.rn"), script).unwrap();

    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let mut registry = HandlerRegistry::with_paths(paths).unwrap();
    registry.discover().unwrap();

    let mut bus = EventBus::new();
    registry.register_all(&mut bus);

    let event = Event::tool_after("test", json!({}));
    let (result, _, errors) = bus.emit(event);

    // First handler should have errored
    assert!(!errors.is_empty(), "First handler should have failed");

    // But second handler should have run
    assert_eq!(
        result.payload["success"],
        json!(true),
        "Second handler should still run"
    );
}

/// Test mixing built-in and Rune handlers
#[tokio::test(flavor = "multi_thread")]
async fn test_mixed_builtin_and_rune_handlers() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    // Write a Rune handler
    let script = r#"
/// Rune handler that adds rune_processed flag
#[handler(event = "tool:after", pattern = "*", priority = 50)]
pub fn rune_processor(ctx, event) {
    let payload = event.payload;
    payload.rune_processed = true;
    event.payload = payload;
    event
}
"#;
    fs::write(handlers_dir.join("rune_handler.rn"), script).unwrap();

    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let mut registry = HandlerRegistry::with_paths(paths).unwrap();
    registry.discover().unwrap();

    let mut bus = EventBus::new();

    // Register built-in hook with higher priority (runs first)
    bus.register(
        Handler::new(
            "builtin_first",
            EventType::ToolAfter,
            "*",
            |_ctx, mut event| {
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("builtin_first".to_string(), json!(true));
                }
                Ok(event)
            },
        )
        .with_priority(10),
    );

    // Register Rune handlers
    registry.register_all(&mut bus);

    // Register built-in hook with lower priority (runs last)
    bus.register(
        Handler::new(
            "builtin_last",
            EventType::ToolAfter,
            "*",
            |_ctx, mut event| {
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("builtin_last".to_string(), json!(true));
                }
                Ok(event)
            },
        )
        .with_priority(100),
    );

    let event = Event::tool_after("test", json!({}));
    let (result, _, errors) = bus.emit(event);

    assert!(errors.is_empty());
    assert_eq!(
        result.payload["builtin_first"],
        json!(true),
        "Built-in first handler should run"
    );
    assert_eq!(
        result.payload["rune_processed"],
        json!(true),
        "Rune handler should run"
    );
    assert_eq!(
        result.payload["builtin_last"],
        json!(true),
        "Built-in last handler should run"
    );
}

/// Test handler manager thread safety
#[tokio::test(flavor = "multi_thread")]
async fn test_handler_manager_concurrent_access() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    let script = r#"
#[handler(event = "tool:after", pattern = "*")]
pub fn concurrent_handler(ctx, event) {
    event
}
"#;
    fs::write(handlers_dir.join("hook.rn"), script).unwrap();

    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let manager = HandlerManager::with_paths(paths).unwrap();
    manager.discover().unwrap();

    // Access from multiple threads
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let count = manager.count();
            std::thread::spawn(move || count)
        })
        .collect();

    for handle in handles {
        let count = handle.join().unwrap();
        assert_eq!(count, 1);
    }
}

/// Test event context metadata passing to hooks
#[tokio::test(flavor = "multi_thread")]
async fn test_handler_receives_context() {
    let temp = TempDir::new().unwrap();
    let handlers_dir = temp.path().join("handlers");
    fs::create_dir_all(&handlers_dir).unwrap();

    // Hook that reads context and adds to payload
    let script = r#"
#[handler(event = "tool:after", pattern = "*")]
pub fn context_reader(ctx, event) {
    // Copy context key to payload if present
    if ctx.contains_key("request_id") {
        let payload = event.payload;
        payload.from_context = ctx["request_id"];
        event.payload = payload;
    }
    event
}
"#;
    fs::write(handlers_dir.join("ctx_handler.rn"), script).unwrap();

    let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
    let mut registry = HandlerRegistry::with_paths(paths).unwrap();
    registry.discover().unwrap();

    let mut bus = EventBus::new();

    // Add a handler that sets context before the Rune handler
    bus.register(
        Handler::new("context_setter", EventType::ToolAfter, "*", |ctx, event| {
            ctx.set("request_id", json!("req-123"));
            Ok(event)
        })
        .with_priority(1), // Run first
    );

    registry.register_all(&mut bus);

    let event = Event::tool_after("test", json!({}));
    let (result, _, errors) = bus.emit(event);

    assert!(errors.is_empty());
    assert_eq!(result.payload["from_context"], json!("req-123"));
}
