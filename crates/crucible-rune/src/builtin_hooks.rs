//! Built-in hooks for the event system
//!
//! These hooks are implemented in Rust for performance and are registered
//! alongside Rune script hooks. They can be enabled/disabled via configuration.
//!
//! ## Available Built-in Hooks
//!
//! - `TestFilterHook` - Filters verbose test output for LLM consumption
//! - `ToonTransformHook` - Transforms tool results to TOON format
//! - `EventEmitHook` - Publishes events to external consumers (webhooks, etc.)
//!
//! ## Configuration
//!
//! ```toml
//! [hooks.builtin.test_filter]
//! enabled = true
//! pattern = "just_test*"
//!
//! [hooks.builtin.toon_transform]
//! enabled = true
//! pattern = "*"
//! ```

use crate::event_bus::{Event, EventContext, Handler, HandlerError, HandlerResult, EventType};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tracing::debug;

/// Configuration for built-in hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinHooksConfig {
    /// Test filter hook configuration
    #[serde(default)]
    pub test_filter: HookToggle,

    /// TOON transform hook configuration
    #[serde(default)]
    pub toon_transform: HookToggle,

    /// Event emit hook configuration
    #[serde(default)]
    pub event_emit: EventEmitConfig,
}

impl Default for BuiltinHooksConfig {
    fn default() -> Self {
        Self {
            test_filter: HookToggle {
                enabled: true,
                pattern: "just_test*".to_string(),
                priority: 10,
            },
            toon_transform: HookToggle {
                enabled: false, // Disabled by default - needs tq integration
                pattern: "*".to_string(),
                priority: 50,
            },
            event_emit: EventEmitConfig::default(),
        }
    }
}

/// Simple toggle for a built-in hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookToggle {
    /// Whether the hook is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Pattern for matching tool names (glob-style)
    #[serde(default = "default_pattern")]
    pub pattern: String,

    /// Priority for handler execution (lower = earlier)
    #[serde(default = "default_priority")]
    pub priority: i64,
}

fn default_true() -> bool { true }
fn default_pattern() -> String { "*".to_string() }
fn default_priority() -> i64 { 100 }

impl Default for HookToggle {
    fn default() -> Self {
        Self {
            enabled: true,
            pattern: "*".to_string(),
            priority: 100,
        }
    }
}

/// Configuration for the event emit hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEmitConfig {
    /// Whether the hook is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Pattern for matching tool names
    #[serde(default = "default_pattern")]
    pub pattern: String,

    /// Priority for handler execution
    #[serde(default = "default_priority")]
    pub priority: i64,

    /// Webhook URL to POST events to (if any)
    #[serde(default)]
    pub webhook_url: Option<String>,
}

impl Default for EventEmitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: "*".to_string(),
            priority: 200, // Run last
            webhook_url: None,
        }
    }
}

/// Create a test filter hook that processes tool:after events
///
/// Extracts summary and error information from test framework output,
/// reducing verbose logs to essential information for LLMs.
pub fn create_test_filter_hook(config: &HookToggle) -> Handler {
    let pattern = config.pattern.clone();
    let priority = config.priority;

    Handler::new(
        "builtin:test_filter",
        EventType::ToolAfter,
        pattern,
        move |_ctx, mut event| {
            // Get the tool result content from payload
            let payload = &event.payload;

            // Look for text content in the result
            if let Some(content) = payload.get("content") {
                if let Some(content_array) = content.as_array() {
                    // Find text blocks and filter them
                    let filtered_content: Vec<JsonValue> = content_array
                        .iter()
                        .map(|block| {
                            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                    if let Some(filtered) = filter_test_output_native(text) {
                                        return json!({
                                            "type": "text",
                                            "text": filtered
                                        });
                                    }
                                }
                            }
                            block.clone()
                        })
                        .collect();

                    // Update payload with filtered content
                    if let Some(obj) = event.payload.as_object_mut() {
                        obj.insert("content".to_string(), json!(filtered_content));
                    }
                }
            }

            Ok(event)
        },
    )
    .with_priority(priority)
    .with_enabled(config.enabled)
}

/// Create a TOON transform hook
///
/// Transforms tool results to TOON (Terse Object/Outline Notation) format
/// for token-efficient responses.
pub fn create_toon_transform_hook(config: &HookToggle) -> Handler {
    let pattern = config.pattern.clone();
    let priority = config.priority;

    Handler::new(
        "builtin:toon_transform",
        EventType::ToolAfter,
        pattern,
        move |_ctx, event| {
            // TOON transformation would use the tq crate
            // For now, just pass through - full implementation requires tq integration
            debug!("TOON transform hook triggered for {}", event.identifier);
            Ok(event)
        },
    )
    .with_priority(priority)
    .with_enabled(config.enabled)
}

/// Create an event emit hook that publishes events to external consumers
///
/// Can be used for:
/// - Logging/auditing tool calls
/// - Triggering webhooks
/// - Publishing to message queues
pub fn create_event_emit_hook(config: &EventEmitConfig) -> Handler {
    let pattern = config.pattern.clone();
    let priority = config.priority;
    let webhook_url = config.webhook_url.clone();

    Handler::new(
        "builtin:event_emit",
        EventType::ToolAfter,
        pattern,
        move |ctx, event| {
            // Emit a custom event for external consumption
            ctx.emit_custom("audit:tool_executed", json!({
                "tool_name": event.identifier,
                "event_type": event.event_type.as_str(),
                "timestamp_ms": event.timestamp_ms,
                "source": event.source,
            }));

            // If webhook URL is configured, note it in context for later processing
            if let Some(ref url) = webhook_url {
                ctx.set("webhook_url", json!(url));
            }

            Ok(event)
        },
    )
    .with_priority(priority)
    .with_enabled(config.enabled)
}

/// Register all enabled built-in hooks on an EventBus
pub fn register_builtin_hooks(bus: &mut crate::event_bus::EventBus, config: &BuiltinHooksConfig) {
    if config.test_filter.enabled {
        bus.register(create_test_filter_hook(&config.test_filter));
        debug!("Registered builtin:test_filter hook");
    }

    if config.toon_transform.enabled {
        bus.register(create_toon_transform_hook(&config.toon_transform));
        debug!("Registered builtin:toon_transform hook");
    }

    if config.event_emit.enabled {
        bus.register(create_event_emit_hook(&config.event_emit));
        debug!("Registered builtin:event_emit hook");
    }
}

// ============================================================================
// Native test output filtering (ported from crucible-tools/src/output_filter.rs)
// ============================================================================

/// Filter test output to extract only summary and error information
///
/// Returns `Some(filtered)` if the output was filtered, `None` if it should
/// pass through unchanged (not recognized as test output).
fn filter_test_output_native(output: &str) -> Option<String> {
    if is_cargo_test(output) {
        Some(filter_cargo_test(output))
    } else if is_pytest(output) {
        Some(filter_pytest(output))
    } else if is_jest(output) {
        Some(filter_jest(output))
    } else if is_go_test(output) {
        Some(filter_go_test(output))
    } else if is_rspec_or_mix(output) {
        Some(filter_rspec_mix(output))
    } else {
        None
    }
}

fn is_cargo_test(output: &str) -> bool {
    output.contains("test result:") ||
    (output.contains("running ") && output.contains(" test"))
}

fn is_pytest(output: &str) -> bool {
    output.contains("passed in ") ||
    output.contains("failed in ") ||
    (output.contains("=====") && (output.contains("passed") || output.contains("failed")))
}

fn is_jest(output: &str) -> bool {
    output.contains("Test Suites:") ||
    (output.contains("Tests:") && (output.contains("passed") || output.contains("failed")))
}

fn is_go_test(output: &str) -> bool {
    output.starts_with("PASS") ||
    output.starts_with("FAIL") ||
    output.contains("\nPASS\n") ||
    output.contains("\nFAIL\n") ||
    output.contains("\nok \t") ||
    output.contains("\nFAIL\t")
}

fn is_rspec_or_mix(output: &str) -> bool {
    (output.contains(" examples,") && output.contains(" failure")) ||
    (output.contains(" tests,") && output.contains(" failure")) ||
    output.contains("Finished in ")
}

fn filter_cargo_test(output: &str) -> String {
    let mut summary_lines = Vec::new();
    let mut in_failures = false;
    let mut failure_lines = Vec::new();

    for line in output.lines() {
        if line.contains("failures:") {
            in_failures = true;
            continue;
        }
        if in_failures && line.trim().is_empty() {
            in_failures = false;
        }

        if in_failures && !line.trim().is_empty() && !line.contains("---- ") {
            failure_lines.push(line);
        }

        if line.starts_with("running ") && line.contains(" test") {
            summary_lines.push(line.to_string());
        }

        if line.contains("test result:") {
            summary_lines.push(line.to_string());
        }

        if line.starts_with("error[") || line.starts_with("error:") {
            summary_lines.push(line.to_string());
        }

        if line.contains("warning:") && line.contains("generated") {
            summary_lines.push(line.to_string());
        }
    }

    if !failure_lines.is_empty() {
        summary_lines.push("\nFailures:".to_string());
        for line in failure_lines.iter().take(20) {
            summary_lines.push(format!("  {}", line));
        }
        if failure_lines.len() > 20 {
            summary_lines.push(format!("  ... and {} more", failure_lines.len() - 20));
        }
    }

    summary_lines.join("\n")
}

fn filter_pytest(output: &str) -> String {
    let mut summary_lines = Vec::new();
    let mut in_failures = false;
    let mut failure_lines = Vec::new();

    for line in output.lines() {
        if line.contains("= FAILURES =") || line.contains("= ERRORS =") {
            in_failures = true;
            summary_lines.push(line.to_string());
            continue;
        }

        if in_failures && line.starts_with("=") && !line.contains("FAILURES") && !line.contains("ERRORS") {
            in_failures = false;
        }

        if in_failures {
            failure_lines.push(line.to_string());
            if failure_lines.len() >= 30 {
                in_failures = false;
            }
        }

        if line.starts_with("=") && (line.contains("passed") || line.contains("failed") || line.contains("error")) {
            summary_lines.push(line.to_string());
        }

        if line.starts_with("FAILED ") || line.starts_with("ERROR ") {
            summary_lines.push(line.to_string());
        }
    }

    if !failure_lines.is_empty() {
        summary_lines.extend(failure_lines.into_iter().take(30));
    }

    summary_lines.join("\n")
}

fn filter_jest(output: &str) -> String {
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim_start();

        if line.contains("Test Suites:") {
            summary_lines.push(line.to_string());
        }

        if line.contains("Tests:") && (line.contains("passed") || line.contains("failed")) {
            summary_lines.push(line.to_string());
        }

        if line.contains("Snapshots:") {
            summary_lines.push(line.to_string());
        }

        if line.contains("Time:") {
            summary_lines.push(line.to_string());
        }

        if trimmed.starts_with("PASS ") || trimmed.starts_with("FAIL ") {
            summary_lines.push(line.to_string());
        }

        if line.contains("● ") {
            summary_lines.push(line.to_string());
        }
    }

    summary_lines.join("\n")
}

fn filter_go_test(output: &str) -> String {
    let mut summary_lines = Vec::new();

    for line in output.lines() {
        if line.starts_with("ok \t") || line.starts_with("ok  ") {
            summary_lines.push(line.to_string());
        }
        if line.starts_with("FAIL\t") || line.starts_with("FAIL ") {
            if !line.starts_with("FAIL:") {
                summary_lines.push(line.to_string());
            }
        }

        if line == "PASS" || line == "FAIL" {
            summary_lines.push(line.to_string());
        }

        if line.starts_with("--- FAIL:") {
            summary_lines.push(line.to_string());
        }

        if line.contains("FAIL:") || line.starts_with("    Error:") {
            summary_lines.push(line.to_string());
        }
    }

    summary_lines.join("\n")
}

fn filter_rspec_mix(output: &str) -> String {
    let mut summary_lines = Vec::new();
    let mut in_failures = false;
    let mut failure_lines = Vec::new();

    for line in output.lines() {
        if line.contains("Failures:") {
            in_failures = true;
            summary_lines.push(line.to_string());
            continue;
        }

        if in_failures && line.starts_with("Finished in ") {
            in_failures = false;
        }

        if in_failures {
            failure_lines.push(line.to_string());
            if failure_lines.len() >= 30 {
                in_failures = false;
            }
        }

        if line.starts_with("Finished in ") {
            summary_lines.push(line.to_string());
        }

        if line.contains(" examples,") && line.contains(" failure") {
            summary_lines.push(line.to_string());
        }

        if line.contains(" tests,") && line.contains(" failure") {
            summary_lines.push(line.to_string());
        }
    }

    if !failure_lines.is_empty() {
        summary_lines.extend(failure_lines.into_iter().take(30));
    }

    summary_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;

    #[test]
    fn test_default_config() {
        let config = BuiltinHooksConfig::default();
        assert!(config.test_filter.enabled);
        assert!(!config.toon_transform.enabled);
        assert!(!config.event_emit.enabled);
    }

    #[test]
    fn test_register_builtin_hooks() {
        let mut bus = EventBus::new();
        let config = BuiltinHooksConfig::default();

        register_builtin_hooks(&mut bus, &config);

        // Should have registered test_filter since it's enabled by default
        assert!(bus.count_handlers(EventType::ToolAfter) >= 1);
    }

    #[test]
    fn test_test_filter_hook_filters_cargo_output() {
        let config = HookToggle {
            enabled: true,
            pattern: "*".to_string(),
            priority: 10,
        };

        let hook = create_test_filter_hook(&config);
        let mut ctx = EventContext::new();

        let cargo_output = r#"running 5 tests
test foo::test_one ... ok
test foo::test_two ... ok
test result: ok. 5 passed; 0 failed"#;

        let event = Event::tool_after("just_test", json!({
            "content": [{
                "type": "text",
                "text": cargo_output
            }]
        }));

        let result = hook.handle(&mut ctx, event).unwrap();

        // Verify content was filtered
        let content = result.payload.get("content").unwrap();
        let text = content[0].get("text").unwrap().as_str().unwrap();

        assert!(text.contains("running 5 tests"));
        assert!(text.contains("test result: ok. 5 passed"));
        assert!(!text.contains("test foo::test_one")); // Individual tests filtered out
    }

    #[test]
    fn test_test_filter_hook_passes_non_test_output() {
        let config = HookToggle {
            enabled: true,
            pattern: "*".to_string(),
            priority: 10,
        };

        let hook = create_test_filter_hook(&config);
        let mut ctx = EventContext::new();

        let regular_output = "Hello, this is not test output.";

        let event = Event::tool_after("some_tool", json!({
            "content": [{
                "type": "text",
                "text": regular_output
            }]
        }));

        let result = hook.handle(&mut ctx, event).unwrap();

        // Non-test output should pass through unchanged
        let content = result.payload.get("content").unwrap();
        let text = content[0].get("text").unwrap().as_str().unwrap();
        assert_eq!(text, regular_output);
    }

    #[test]
    fn test_event_emit_hook_emits_audit_event() {
        let config = EventEmitConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 200,
            webhook_url: Some("https://example.com/webhook".to_string()),
        };

        let hook = create_event_emit_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_after("test_tool", json!({}));

        let _result = hook.handle(&mut ctx, event).unwrap();

        // Should have emitted an audit event
        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].identifier, "audit:tool_executed");

        // Should have set webhook URL in context
        assert_eq!(ctx.get("webhook_url"), Some(&json!("https://example.com/webhook")));
    }

    #[test]
    fn test_native_filter_cargo_test() {
        let input = r#"running 42 tests
test foo::test_one ... ok
test result: ok. 42 passed; 0 failed"#;

        let filtered = filter_test_output_native(input).unwrap();

        assert!(filtered.contains("running 42 tests"));
        assert!(filtered.contains("test result: ok. 42 passed"));
        assert!(!filtered.contains("test_one"));
    }

    #[test]
    fn test_native_filter_pytest() {
        let input = r#"test_module.py::test_one PASSED
============================== 25 passed in 1.23s ==============================
"#;

        let filtered = filter_test_output_native(input).unwrap();

        assert!(filtered.contains("25 passed in 1.23s"));
        assert!(!filtered.contains("test_one PASSED"));
    }

    #[test]
    fn test_native_filter_non_test() {
        let input = "Just regular output";
        assert!(filter_test_output_native(input).is_none());
    }
}
