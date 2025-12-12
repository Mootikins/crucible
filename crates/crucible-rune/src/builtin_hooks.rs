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

use crate::event_bus::{EventType, Handler};
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

    /// Tool selector hook configuration
    #[serde(default)]
    pub tool_selector: ToolSelectorConfig,

    /// Recipe enrichment hook configuration
    #[serde(default)]
    pub recipe_enrichment: HookToggle,
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
            tool_selector: ToolSelectorConfig::default(),
            recipe_enrichment: HookToggle {
                enabled: true,
                pattern: "just_*".to_string(),
                priority: 5, // Run early, before other enrichment
            },
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

fn default_true() -> bool {
    true
}
fn default_pattern() -> String {
    "*".to_string()
}
fn default_priority() -> i64 {
    100
}

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

/// Configuration for the tool selector hook
///
/// This hook runs on `tool:discovered` events and can:
/// - Filter tools by whitelist/blacklist patterns
/// - Add namespace prefixes to tool names
/// - Cancel discovery of unwanted tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSelectorConfig {
    /// Whether the hook is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Pattern for matching tool names (glob-style)
    #[serde(default = "default_pattern")]
    pub pattern: String,

    /// Priority for handler execution
    #[serde(default = "default_selector_priority")]
    pub priority: i64,

    /// Whitelist of allowed tool patterns (if set, only matching tools pass through)
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Blacklist of blocked tool patterns (matching tools are cancelled)
    #[serde(default)]
    pub blocked_tools: Option<Vec<String>>,

    /// Prefix to add to tool names
    #[serde(default)]
    pub prefix: Option<String>,

    /// Suffix to add to tool names
    #[serde(default)]
    pub suffix: Option<String>,
}

fn default_selector_priority() -> i64 {
    5
} // Run very early

impl Default for ToolSelectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: "*".to_string(),
            priority: 5, // Run early to filter before other hooks
            allowed_tools: None,
            blocked_tools: None,
            prefix: None,
            suffix: None,
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
            ctx.emit_custom(
                "audit:tool_executed",
                json!({
                    "tool_name": event.identifier,
                    "event_type": event.event_type.as_str(),
                    "timestamp_ms": event.timestamp_ms,
                    "source": event.source,
                }),
            );

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

/// Create a tool selector hook that filters and namespaces discovered tools
///
/// This hook runs on `tool:discovered` events and can:
/// - Block tools matching blacklist patterns
/// - Only allow tools matching whitelist patterns
/// - Add prefixes/suffixes to tool names
///
/// When a tool is blocked, the event is cancelled and the tool won't be registered.
pub fn create_tool_selector_hook(config: &ToolSelectorConfig) -> Handler {
    let pattern = config.pattern.clone();
    let priority = config.priority;
    let allowed_tools = config.allowed_tools.clone();
    let blocked_tools = config.blocked_tools.clone();
    let prefix = config.prefix.clone();
    let suffix = config.suffix.clone();

    Handler::new(
        "builtin:tool_selector",
        EventType::ToolDiscovered,
        pattern,
        move |_ctx, mut event| {
            let tool_name = event.identifier.clone();

            // Check blacklist first (takes precedence)
            if let Some(ref blocked) = blocked_tools {
                for pattern in blocked {
                    if glob_match_simple(pattern, &tool_name) {
                        debug!("Tool '{}' blocked by pattern '{}'", tool_name, pattern);
                        event.cancel();
                        return Ok(event);
                    }
                }
            }

            // Check whitelist (if set, tool must match at least one pattern)
            if let Some(ref allowed) = allowed_tools {
                let matches_any = allowed.iter().any(|p| glob_match_simple(p, &tool_name));
                if !matches_any {
                    debug!("Tool '{}' not in whitelist, blocking", tool_name);
                    event.cancel();
                    return Ok(event);
                }
            }

            // Apply namespace transformations
            let mut new_name = tool_name.clone();
            if let Some(ref p) = prefix {
                new_name = format!("{}{}", p, new_name);
            }
            if let Some(ref s) = suffix {
                new_name = format!("{}{}", new_name, s);
            }

            // Update the event if name changed
            if new_name != tool_name {
                debug!("Renaming tool '{}' to '{}'", tool_name, new_name);
                event.identifier = new_name.clone();

                // Also update the name in the payload if present
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("original_name".to_string(), json!(tool_name));
                    obj.insert("name".to_string(), json!(new_name));
                }
            }

            Ok(event)
        },
    )
    .with_priority(priority)
    .with_enabled(config.enabled)
}

/// Simple glob pattern matching for tool selector
///
/// Supports:
/// - `*` - matches any sequence of characters
/// - `?` - matches exactly one character
fn glob_match_simple(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    glob_match_recursive(&pattern_chars, &text_chars, 0, 0)
}

fn glob_match_recursive(pattern: &[char], text: &[char], pi: usize, ti: usize) -> bool {
    if pi == pattern.len() && ti == text.len() {
        return true;
    }

    if pi == pattern.len() {
        return false;
    }

    match pattern[pi] {
        '*' => {
            for i in ti..=text.len() {
                if glob_match_recursive(pattern, text, pi + 1, i) {
                    return true;
                }
            }
            false
        }
        '?' => {
            if ti < text.len() {
                glob_match_recursive(pattern, text, pi + 1, ti + 1)
            } else {
                false
            }
        }
        c => {
            if ti < text.len() && text[ti] == c {
                glob_match_recursive(pattern, text, pi + 1, ti + 1)
            } else {
                false
            }
        }
    }
}

/// Create a recipe enrichment hook that categorizes Just recipes
///
/// This hook runs on `tool:discovered` events for Just recipes and automatically
/// adds category, tags, and priority based on the recipe name.
///
/// Categorization logic:
/// - test* → category: "testing", tags: ["ci"]
/// - build*, release* → category: "build"
/// - fmt*, clippy*, check* → category: "quality", tags: ["ci", "quick"]
/// - docs* → category: "documentation"
/// - clean → category: "maintenance"
/// - deploy* → category: "deploy"
/// - etc.
pub fn create_recipe_enrichment_hook(config: &HookToggle) -> Handler {
    let pattern = config.pattern.clone();
    let priority = config.priority;

    Handler::new(
        "builtin:recipe_enrichment",
        EventType::ToolDiscovered,
        pattern,
        move |_ctx, mut event| {
            // Extract recipe name from tool name (strip just_ prefix)
            let tool_name = event.identifier.clone();
            let recipe_name = tool_name
                .strip_prefix("just_")
                .unwrap_or(&tool_name)
                .replace('_', "-");

            // Categorize by name
            let category = categorize_recipe(&recipe_name);

            // Determine tags based on category and name
            let tags = determine_tags(&recipe_name, category);

            // Determine priority
            let priority = determine_priority(&recipe_name, category);

            // Add enrichment to payload
            if let Some(obj) = event.payload.as_object_mut() {
                obj.insert("category".to_string(), json!(category));
                obj.insert("tags".to_string(), json!(tags));
                obj.insert("priority".to_string(), json!(priority));
            }

            debug!(
                "Enriched recipe '{}' with category='{}', tags={:?}, priority={}",
                recipe_name, category, tags, priority
            );

            Ok(event)
        },
    )
    .with_priority(priority)
    .with_enabled(config.enabled)
}

/// Categorize a recipe by name
fn categorize_recipe(name: &str) -> &'static str {
    // Use the existing categorize_by_name_impl from rune_types
    crate::rune_types::categorize_by_name_impl(name)
}

/// Determine tags for a recipe based on name and category
fn determine_tags(name: &str, category: &str) -> Vec<String> {
    let mut tags = Vec::new();

    // Add category-specific tags
    match category {
        "testing" => {
            tags.push("ci".to_string());
            if name.contains("unit") || name.contains("quick") {
                tags.push("quick".to_string());
            }
        }
        "quality" => {
            tags.push("ci".to_string());
            tags.push("quick".to_string());
        }
        "build" => {
            tags.push("build".to_string());
        }
        "ci" => {
            tags.push("ci".to_string());
        }
        _ => {}
    }

    tags
}

/// Determine priority for a recipe based on name and category
fn determine_priority(name: &str, category: &str) -> i32 {
    match category {
        "testing" => {
            if name.contains("unit") || name.contains("quick") {
                10 // High priority for quick tests
            } else {
                20 // Normal priority for other tests
            }
        }
        "quality" => 15, // High priority for quality checks
        "build" => {
            if name.contains("release") {
                40 // Lower priority for release builds
            } else {
                30 // Normal priority for dev builds
            }
        }
        "ci" => 5,      // Very high priority
        "default" => 1, // Highest priority (usually help/list)
        _ => 50,        // Default priority
    }
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

    if config.tool_selector.enabled {
        bus.register(create_tool_selector_hook(&config.tool_selector));
        debug!("Registered builtin:tool_selector hook");
    }

    if config.recipe_enrichment.enabled {
        bus.register(create_recipe_enrichment_hook(&config.recipe_enrichment));
        debug!("Registered builtin:recipe_enrichment hook");
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
    output.contains("test result:") || (output.contains("running ") && output.contains(" test"))
}

fn is_pytest(output: &str) -> bool {
    output.contains("passed in ")
        || output.contains("failed in ")
        || (output.contains("=====") && (output.contains("passed") || output.contains("failed")))
}

fn is_jest(output: &str) -> bool {
    output.contains("Test Suites:")
        || (output.contains("Tests:") && (output.contains("passed") || output.contains("failed")))
}

fn is_go_test(output: &str) -> bool {
    output.starts_with("PASS")
        || output.starts_with("FAIL")
        || output.contains("\nPASS\n")
        || output.contains("\nFAIL\n")
        || output.contains("\nok \t")
        || output.contains("\nFAIL\t")
}

fn is_rspec_or_mix(output: &str) -> bool {
    (output.contains(" examples,") && output.contains(" failure"))
        || (output.contains(" tests,") && output.contains(" failure"))
        || output.contains("Finished in ")
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

        if in_failures
            && line.starts_with("=")
            && !line.contains("FAILURES")
            && !line.contains("ERRORS")
        {
            in_failures = false;
        }

        if in_failures {
            failure_lines.push(line.to_string());
            if failure_lines.len() >= 30 {
                in_failures = false;
            }
        }

        if line.starts_with("=")
            && (line.contains("passed") || line.contains("failed") || line.contains("error"))
        {
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
        if (line.starts_with("FAIL\t") || line.starts_with("FAIL ")) && !line.starts_with("FAIL:") {
            summary_lines.push(line.to_string());
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
    use crate::event_bus::{Event, EventBus, EventContext};

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

        let event = Event::tool_after(
            "just_test",
            json!({
                "content": [{
                    "type": "text",
                    "text": cargo_output
                }]
            }),
        );

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

        let event = Event::tool_after(
            "some_tool",
            json!({
                "content": [{
                    "type": "text",
                    "text": regular_output
                }]
            }),
        );

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
        assert_eq!(
            ctx.get("webhook_url"),
            Some(&json!("https://example.com/webhook"))
        );
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

    // ============================================================================
    // Tool Selector Hook Tests
    // ============================================================================

    #[test]
    fn test_tool_selector_default_config() {
        let config = ToolSelectorConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.pattern, "*");
        assert_eq!(config.priority, 5);
        assert!(config.allowed_tools.is_none());
        assert!(config.blocked_tools.is_none());
    }

    #[test]
    fn test_tool_selector_pass_through_no_filters() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: None,
            blocked_tools: None,
            prefix: None,
            suffix: None,
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("my_tool", json!({"name": "my_tool"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert!(!result.is_cancelled());
        assert_eq!(result.identifier, "my_tool");
    }

    #[test]
    fn test_tool_selector_blacklist_blocks_tool() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: None,
            blocked_tools: Some(vec!["dangerous_*".to_string(), "delete_*".to_string()]),
            prefix: None,
            suffix: None,
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        // Should be blocked
        let event = Event::tool_discovered("dangerous_tool", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(result.is_cancelled());

        // Should be blocked
        let event = Event::tool_discovered("delete_repo", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(result.is_cancelled());

        // Should pass through
        let event = Event::tool_discovered("safe_tool", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(!result.is_cancelled());
    }

    #[test]
    fn test_tool_selector_whitelist_filters_tools() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: Some(vec!["search_*".to_string(), "get_*".to_string()]),
            blocked_tools: None,
            prefix: None,
            suffix: None,
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        // Should pass (matches whitelist)
        let event = Event::tool_discovered("search_code", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(!result.is_cancelled());

        // Should pass (matches whitelist)
        let event = Event::tool_discovered("get_user", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(!result.is_cancelled());

        // Should be blocked (not in whitelist)
        let event = Event::tool_discovered("delete_user", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(result.is_cancelled());
    }

    #[test]
    fn test_tool_selector_blacklist_overrides_whitelist() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: Some(vec!["*".to_string()]),
            blocked_tools: Some(vec!["dangerous".to_string()]),
            prefix: None,
            suffix: None,
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        // Should be blocked (blacklist overrides whitelist)
        let event = Event::tool_discovered("dangerous", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(result.is_cancelled());

        // Should pass
        let event = Event::tool_discovered("safe", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();
        assert!(!result.is_cancelled());
    }

    #[test]
    fn test_tool_selector_applies_prefix() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: None,
            blocked_tools: None,
            prefix: Some("gh_".to_string()),
            suffix: None,
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("search_code", json!({"name": "search_code"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert!(!result.is_cancelled());
        assert_eq!(result.identifier, "gh_search_code");
        assert_eq!(result.payload["name"], json!("gh_search_code"));
        assert_eq!(result.payload["original_name"], json!("search_code"));
    }

    #[test]
    fn test_tool_selector_applies_suffix() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: None,
            blocked_tools: None,
            prefix: None,
            suffix: Some("_v2".to_string()),
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("search", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert!(!result.is_cancelled());
        assert_eq!(result.identifier, "search_v2");
    }

    #[test]
    fn test_tool_selector_applies_prefix_and_suffix() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "*".to_string(),
            priority: 5,
            allowed_tools: None,
            blocked_tools: None,
            prefix: Some("ns_".to_string()),
            suffix: Some("_tool".to_string()),
        };

        let hook = create_tool_selector_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("search", json!({}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert!(!result.is_cancelled());
        assert_eq!(result.identifier, "ns_search_tool");
    }

    #[test]
    fn test_glob_match_simple() {
        assert!(glob_match_simple("*", "anything"));
        assert!(glob_match_simple("*", ""));
        assert!(glob_match_simple("foo*", "foobar"));
        assert!(glob_match_simple("foo*", "foo"));
        assert!(!glob_match_simple("foo*", "bar"));
        assert!(glob_match_simple("*bar", "foobar"));
        assert!(glob_match_simple("foo*bar", "fooXXXbar"));
        assert!(glob_match_simple("foo?bar", "fooXbar"));
        assert!(!glob_match_simple("foo?bar", "fooXXbar"));
        assert!(glob_match_simple("search_*", "search_repositories"));
        assert!(glob_match_simple("gh_*", "gh_search_code"));
    }

    #[test]
    fn test_tool_selector_config_serialization() {
        let config = ToolSelectorConfig {
            enabled: true,
            pattern: "upstream:*".to_string(),
            priority: 10,
            allowed_tools: Some(vec!["search_*".to_string()]),
            blocked_tools: Some(vec!["delete_*".to_string()]),
            prefix: Some("gh_".to_string()),
            suffix: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ToolSelectorConfig = serde_json::from_str(&json).unwrap();

        assert!(parsed.enabled);
        assert_eq!(parsed.prefix, Some("gh_".to_string()));
        assert_eq!(parsed.allowed_tools, Some(vec!["search_*".to_string()]));
    }

    // ============================================================================
    // Recipe Enrichment Hook Tests
    // ============================================================================

    #[test]
    fn test_recipe_enrichment_hook_categorizes_test() {
        let config = HookToggle {
            enabled: true,
            pattern: "just_*".to_string(),
            priority: 5,
        };

        let hook = create_recipe_enrichment_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("just_test", json!({"name": "just_test"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert!(!result.is_cancelled());
        assert_eq!(result.payload["category"], json!("testing"));
        assert!(result.payload["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("ci")));
        assert_eq!(result.payload["priority"], json!(20)); // Default test priority
    }

    #[test]
    fn test_recipe_enrichment_hook_categorizes_build() {
        let config = HookToggle {
            enabled: true,
            pattern: "just_*".to_string(),
            priority: 5,
        };

        let hook = create_recipe_enrichment_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("just_build", json!({"name": "just_build"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert_eq!(result.payload["category"], json!("build"));
        assert!(result.payload["tags"]
            .as_array()
            .unwrap()
            .contains(&json!("build")));
        assert_eq!(result.payload["priority"], json!(30));
    }

    #[test]
    fn test_recipe_enrichment_hook_categorizes_quality() {
        let config = HookToggle {
            enabled: true,
            pattern: "just_*".to_string(),
            priority: 5,
        };

        let hook = create_recipe_enrichment_hook(&config);
        let mut ctx = EventContext::new();

        let event = Event::tool_discovered("just_fmt", json!({"name": "just_fmt"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert_eq!(result.payload["category"], json!("quality"));
        let tags = result.payload["tags"].as_array().unwrap();
        assert!(tags.contains(&json!("ci")));
        assert!(tags.contains(&json!("quick")));
        assert_eq!(result.payload["priority"], json!(15));
    }

    #[test]
    fn test_recipe_enrichment_hook_quick_test_priority() {
        let config = HookToggle {
            enabled: true,
            pattern: "just_*".to_string(),
            priority: 5,
        };

        let hook = create_recipe_enrichment_hook(&config);
        let mut ctx = EventContext::new();

        // Quick test should have higher priority than normal test
        let event = Event::tool_discovered("just_test_unit", json!({"name": "just_test_unit"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert_eq!(result.payload["category"], json!("testing"));
        let tags = result.payload["tags"].as_array().unwrap();
        assert!(tags.contains(&json!("ci")));
        assert!(tags.contains(&json!("quick")));
        assert_eq!(result.payload["priority"], json!(10)); // Higher priority for quick tests
    }

    #[test]
    fn test_recipe_enrichment_hook_handles_hyphens() {
        let config = HookToggle {
            enabled: true,
            pattern: "just_*".to_string(),
            priority: 5,
        };

        let hook = create_recipe_enrichment_hook(&config);
        let mut ctx = EventContext::new();

        // Underscores in tool name should be converted to hyphens for categorization
        let event = Event::tool_discovered("just_test_crate", json!({"name": "just_test_crate"}));
        let result = hook.handle(&mut ctx, event).unwrap();

        // test-crate should be categorized as testing
        assert_eq!(result.payload["category"], json!("testing"));
    }

    #[test]
    fn test_builtin_config_includes_recipe_enrichment() {
        let config = BuiltinHooksConfig::default();
        assert!(config.recipe_enrichment.enabled);
        assert_eq!(config.recipe_enrichment.pattern, "just_*");
        assert_eq!(config.recipe_enrichment.priority, 5);
    }

    #[test]
    fn test_register_builtin_hooks_includes_recipe_enrichment() {
        let mut bus = EventBus::new();
        let config = BuiltinHooksConfig::default();

        register_builtin_hooks(&mut bus, &config);

        // Should have registered recipe enrichment hook
        let handler = bus.get_handler("builtin:recipe_enrichment");
        assert!(
            handler.is_some(),
            "builtin:recipe_enrichment hook should be registered"
        );
        assert_eq!(handler.unwrap().event_type, EventType::ToolDiscovered);
    }
}
