//! Integration tests for SessionEvent Rune bindings.
//!
//! These tests verify:
//! - Rune scripts can check event types
//! - Rune scripts can construct events
//! - Rune handlers can emit events via ctx.emit()

use crucible_rune::{session_event_module, RuneEventContext, RuneSessionEvent};
use rune::termcolor::{ColorChoice, StandardStream};
use rune::{Context, Diagnostics, Source, Sources, Vm};
use std::sync::Arc;

/// Create a Rune context with the session event module installed.
fn create_rune_context() -> Result<Context, rune::ContextError> {
    let mut context = rune_modules::default_context()?;
    context.install(session_event_module()?)?;
    Ok(context)
}

/// Compile a Rune script.
fn compile_script(
    context: &Context,
    script: &str,
) -> Result<Arc<rune::Unit>, Box<dyn std::error::Error>> {
    let mut sources = Sources::new();
    sources.insert(Source::memory(script)?)?;

    let mut diagnostics = Diagnostics::new();

    let result = rune::prepare(&mut sources)
        .with_context(context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if !diagnostics.is_empty() {
        let mut writer = StandardStream::stderr(ColorChoice::Auto);
        diagnostics.emit(&mut writer, &sources)?;
    }

    Ok(Arc::new(result?))
}

// =============================================================================
// Pattern Matching Tests (6.3.1)
// =============================================================================

#[tokio::test]
async fn test_rune_can_check_event_type() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        pub fn main(event) {
            event.event_type()
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let event = RuneSessionEvent::message_received_impl("Hello".into(), "user".into());

    let output = vm
        .call(["main"], (event,))
        .expect("Script execution failed");

    let result: String = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result, "message_received");
}

#[tokio::test]
async fn test_rune_can_access_event_content() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        pub fn main(event) {
            event.content()
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let event = RuneSessionEvent::message_received_impl("Hello, World!".into(), "user".into());

    let output = vm
        .call(["main"], (event,))
        .expect("Script execution failed");

    let result: Option<String> = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result, Some("Hello, World!".to_string()));
}

#[tokio::test]
async fn test_rune_can_check_is_tool_event() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        pub fn main(event) {
            event.is_tool_event()
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));

    // ToolCompleted should be a tool event
    let mut vm = Vm::new(runtime.clone(), unit.clone());
    let tool_event = RuneSessionEvent::tool_completed_impl("search".into(), "results".into());
    let output = vm
        .call(["main"], (tool_event,))
        .expect("Script execution failed");
    let result: bool = rune::from_value(output).expect("Failed to convert output");
    assert!(result, "ToolCompleted should be a tool event");

    // MessageReceived should NOT be a tool event
    let mut vm = Vm::new(runtime, unit);
    let msg_event = RuneSessionEvent::message_received_impl("Hello".into(), "user".into());
    let output = vm
        .call(["main"], (msg_event,))
        .expect("Script execution failed");
    let result: bool = rune::from_value(output).expect("Failed to convert output");
    assert!(!result, "MessageReceived should not be a tool event");
}

#[tokio::test]
async fn test_rune_conditional_on_event_type() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        pub fn main(event) {
            if event.event_type() == "message_received" {
                "got message"
            } else if event.event_type() == "agent_thinking" {
                "agent is thinking"
            } else {
                "other event"
            }
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));

    let mut vm = Vm::new(runtime.clone(), unit.clone());
    let msg_event = RuneSessionEvent::message_received_impl("Hello".into(), "user".into());
    let output = vm
        .call(["main"], (msg_event,))
        .expect("Script execution failed");
    let result: String = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result, "got message");

    let mut vm = Vm::new(runtime, unit);
    let thinking_event = RuneSessionEvent::agent_thinking_impl("Processing...".into());
    let output = vm
        .call(["main"], (thinking_event,))
        .expect("Script execution failed");
    let result: String = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result, "agent is thinking");
}

// =============================================================================
// Construction Tests (6.3.2)
// =============================================================================

#[tokio::test]
async fn test_rune_can_construct_message_received() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        use crucible::RuneSessionEvent;

        pub fn main() {
            let event = RuneSessionEvent::message_received("Hello from Rune", "rune_script");
            event.event_type()
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let output = vm.call(["main"], ()).expect("Script execution failed");

    let event_type: String = rune::from_value(output).expect("Failed to convert");
    assert_eq!(event_type, "message_received");
}

#[tokio::test]
async fn test_rune_can_construct_custom_event() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        use crucible::RuneSessionEvent;

        pub fn main() {
            let event = RuneSessionEvent::custom("my_custom_event", #{});
            [event.event_type(), event.custom_name()]
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let output = vm.call(["main"], ()).expect("Script execution failed");

    let result: Vec<rune::Value> = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result.len(), 2);

    let event_type: String = rune::from_value(result[0].clone()).expect("Failed to convert");
    assert_eq!(event_type, "custom");

    let custom_name: Option<String> =
        rune::from_value(result[1].clone()).expect("Failed to convert");
    assert_eq!(custom_name, Some("my_custom_event".to_string()));
}

#[tokio::test]
async fn test_rune_can_construct_tool_events() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        use crucible::RuneSessionEvent;

        pub fn main() {
            let completed = RuneSessionEvent::tool_completed("search", "found 5 results");
            let error = RuneSessionEvent::tool_error("fetch", "timeout", "connection timed out");

            [completed.tool_name(), error.error()]
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let output = vm.call(["main"], ()).expect("Script execution failed");

    let result: Vec<rune::Value> = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result.len(), 2);

    let tool_name: Option<String> = rune::from_value(result[0].clone()).expect("Failed");
    assert_eq!(tool_name, Some("search".to_string()));

    let error: Option<String> = rune::from_value(result[1].clone()).expect("Failed");
    assert_eq!(error, Some("connection timed out".to_string()));
}

// =============================================================================
// Event Emission Tests (6.3.3)
// =============================================================================

#[tokio::test]
async fn test_rune_handler_can_emit_events() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        use crucible::RuneSessionEvent;

        pub fn main(ctx) {
            // Emit a custom event
            let event = RuneSessionEvent::custom("handler_output", #{});
            ctx.emit(event);

            // Also emit using emit_custom
            ctx.emit_custom("another_output", #{"key": "value"});

            ctx.emitted_count()
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let ctx = RuneEventContext::new();

    let output = vm
        .call(["main"], (ctx,))
        .expect("Script execution failed");

    let count: i64 = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(count, 2, "Should have emitted 2 events");
}

#[tokio::test]
async fn test_rune_context_metadata() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        pub fn main(ctx) {
            ctx.set("processed", "true");
            ctx.get("processed")
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let ctx = RuneEventContext::new();

    let output = vm
        .call(["main"], (ctx,))
        .expect("Script execution failed");

    let result: Option<String> = rune::from_value(output).expect("Failed");
    assert!(
        result.is_some(),
        "Should have retrieved the metadata value"
    );
}

#[tokio::test]
async fn test_rune_event_display_format() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        pub fn main(event) {
            `${event}`
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let event = RuneSessionEvent::message_received_impl("Hello".into(), "user".into());

    let output = vm
        .call(["main"], (event,))
        .expect("Script execution failed");

    let result: String = rune::from_value(output).expect("Failed to convert output");
    assert!(
        result.contains("SessionEvent"),
        "Display should contain 'SessionEvent', got: {}",
        result
    );
}

#[tokio::test]
async fn test_rune_event_partial_eq() {
    let context = create_rune_context().expect("Failed to create context");

    let script = r#"
        use crucible::RuneSessionEvent;

        pub fn main() {
            let e1 = RuneSessionEvent::message_received("Hello", "user");
            let e2 = RuneSessionEvent::message_received("Different", "other");
            let e3 = RuneSessionEvent::agent_thinking("thinking...");

            // Same type should be equal (we compare by event_type)
            let same_type = e1 == e2;
            // Different types should not be equal
            let diff_type = e1 == e3;

            [same_type, diff_type]
        }
    "#;

    let unit = compile_script(&context, script).expect("Failed to compile");
    let runtime = Arc::new(context.runtime().expect("Failed to get runtime"));
    let mut vm = Vm::new(runtime, unit);

    let output = vm.call(["main"], ()).expect("Script execution failed");

    let result: Vec<bool> = rune::from_value(output).expect("Failed to convert output");
    assert_eq!(result.len(), 2);

    assert!(
        result[0],
        "Events of same type should be equal (by event_type)"
    );
    assert!(
        !result[1],
        "Events of different types should not be equal"
    );
}
