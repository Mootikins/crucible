//! Event pipeline for processing events through Rune plugin hooks
//!
//! The pipeline receives events, finds matching hooks, and executes
//! handler functions in sequence. Handlers can modify events or pass
//! them through unchanged.
//!
//! Note: Rune VMs are not Send, so hook execution is done on a dedicated
//! thread pool via spawn_blocking.

use crate::events::ToolResultEvent;
use crate::plugin_loader::PluginLoader;
use crate::RuneError;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Pipeline for processing events through registered plugin hooks
///
/// This struct is Send + Sync safe. Rune execution happens on blocking threads.
pub struct EventPipeline {
    /// Plugin loader containing registered hooks
    loader: Arc<RwLock<PluginLoader>>,
}

// Safety: EventPipeline only contains Arc<RwLock<PluginLoader>> which is Send + Sync.
// Actual Rune execution happens on spawn_blocking threads.
unsafe impl Send for EventPipeline {}
unsafe impl Sync for EventPipeline {}

impl EventPipeline {
    /// Create a new event pipeline with the given plugin loader
    pub fn new(loader: Arc<RwLock<PluginLoader>>) -> Self {
        Self { loader }
    }

    /// Process a tool result event through all matching hooks
    ///
    /// Hooks are executed in order. Each hook receives the event and can:
    /// - Return the modified event (will be passed to next hook)
    /// - Return null/unit to pass through unchanged
    ///
    /// If a hook errors, the event passes through unchanged and processing continues.
    ///
    /// Note: Rune execution happens on a blocking thread pool since Rune VMs are not Send.
    pub async fn process_tool_result(
        &self,
        event: ToolResultEvent,
    ) -> Result<ToolResultEvent, RuneError> {
        // First, check if there are any matching hooks (this is Send-safe)
        let hooks_info: Vec<(String, std::path::PathBuf)> = {
            let loader = self.loader.read().await;
            let hooks = loader.get_matching_hooks("tool_result", &event.tool_name);
            if hooks.is_empty() {
                debug!("No hooks match tool_result:{}", event.tool_name);
                return Ok(event);
            }
            hooks
                .iter()
                .filter(|h| h.unit.is_some())
                .map(|h| (h.handler_name.clone(), h.plugin_path.clone()))
                .collect()
        };

        if hooks_info.is_empty() {
            return Ok(event);
        }

        debug!(
            "Processing tool_result:{} through {} hooks",
            event.tool_name,
            hooks_info.len()
        );

        // Process hooks by re-loading and executing on each iteration
        // This is less efficient but ensures Send safety
        let mut current_event = event;

        for (handler_name, plugin_path) in hooks_info {
            let loader_clone = self.loader.clone();
            let event_json = serde_json::to_value(&current_event)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            let handler_name_clone = handler_name.clone();
            let plugin_path_clone = plugin_path.clone();

            // Execute on blocking thread since Rune is not Send
            let result = tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let loader = loader_clone.read().await;

                    // Find the hook by handler name and plugin path
                    // (we already filtered by pattern earlier)
                    let hook = loader.hooks().iter().find(|h| {
                        h.handler_name == handler_name_clone && h.plugin_path == plugin_path_clone
                    });

                    let hook = match hook {
                        Some(h) => h,
                        None => return Ok(None),
                    };

                    let unit = match &hook.unit {
                        Some(u) => u,
                        None => return Ok(None),
                    };

                    let event_value = loader.executor().json_to_rune_value(event_json)?;
                    let ctx_json = serde_json::json!({});
                    let ctx_value = loader.executor().json_to_rune_value(ctx_json)?;

                    let result = loader
                        .executor()
                        .call_function(unit, &handler_name_clone, (ctx_value, event_value))
                        .await?;

                    Ok::<_, RuneError>(Some(result))
                })
            })
            .await
            .map_err(|e| RuneError::Execution(format!("Task join error: {}", e)))?;

            match result {
                Ok(Some(returned)) => {
                    if returned.is_null() {
                        debug!("Hook {} returned null, passing through", handler_name);
                        continue;
                    }

                    match serde_json::from_value::<ToolResultEvent>(returned.clone()) {
                        Ok(modified_event) => {
                            debug!("Hook {} modified event", handler_name);
                            current_event = modified_event;
                        }
                        Err(e) => {
                            warn!(
                                "Hook {} returned invalid event ({}), passing through",
                                handler_name, e
                            );
                        }
                    }
                }
                Ok(None) => {
                    debug!("Hook {} not found, skipping", handler_name);
                }
                Err(e) => {
                    warn!(
                        "Hook {} failed ({}), passing through original event",
                        handler_name, e
                    );
                }
            }
        }

        Ok(current_event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ContentBlock;
    use tempfile::TempDir;

    async fn setup_pipeline_with_plugin(plugin_content: &str) -> (EventPipeline, TempDir) {
        let temp = TempDir::new().unwrap();
        let plugin_path = temp.path().join("test_plugin.rn");
        std::fs::write(&plugin_path, plugin_content).unwrap();

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        let pipeline = EventPipeline::new(Arc::new(RwLock::new(loader)));
        (pipeline, temp)
    }

    fn make_event(tool_name: &str, text: &str) -> ToolResultEvent {
        ToolResultEvent {
            tool_name: tool_name.to_string(),
            arguments: serde_json::json!({}),
            is_error: false,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            duration_ms: 100,
        }
    }

    #[tokio::test]
    async fn test_pipeline_no_hooks_passthrough() {
        let temp = TempDir::new().unwrap();
        let loader = PluginLoader::new(temp.path()).unwrap();
        let pipeline = EventPipeline::new(Arc::new(RwLock::new(loader)));

        let event = make_event("some_tool", "original content");
        let result = pipeline.process_tool_result(event.clone()).await.unwrap();

        assert_eq!(result.text_content(), "original content");
    }

    #[tokio::test]
    async fn test_pipeline_hook_modifies_event() {
        let (pipeline, _temp) = setup_pipeline_with_plugin(
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "modify" }] }
}

pub fn modify(ctx, event) {
    event.content = [#{ type: "text", text: "modified!" }];
    event
}
"#,
        )
        .await;

        let event = make_event("any_tool", "original");
        let result = pipeline.process_tool_result(event).await.unwrap();

        assert_eq!(result.text_content(), "modified!");
    }

    #[tokio::test]
    async fn test_pipeline_hook_none_passthrough() {
        let (pipeline, _temp) = setup_pipeline_with_plugin(
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "passthrough" }] }
}

pub fn passthrough(ctx, event) {
    // Return None/null to pass through unchanged
    ()
}
"#,
        )
        .await;

        let event = make_event("tool", "keep me");
        let result = pipeline.process_tool_result(event).await.unwrap();

        assert_eq!(result.text_content(), "keep me");
    }

    #[tokio::test]
    async fn test_pipeline_multiple_hooks_chain() {
        let temp = TempDir::new().unwrap();

        // First plugin adds prefix
        std::fs::write(
            temp.path().join("plugin1.rn"),
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "add_prefix" }] }
}

pub fn add_prefix(ctx, event) {
    let text = event.content[0].text;
    event.content = [#{ type: "text", text: `PREFIX:${text}` }];
    event
}
"#,
        )
        .unwrap();

        // Second plugin adds suffix
        std::fs::write(
            temp.path().join("plugin2.rn"),
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "add_suffix" }] }
}

pub fn add_suffix(ctx, event) {
    let text = event.content[0].text;
    event.content = [#{ type: "text", text: `${text}:SUFFIX` }];
    event
}
"#,
        )
        .unwrap();

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();
        let pipeline = EventPipeline::new(Arc::new(RwLock::new(loader)));

        let event = make_event("tool", "middle");
        let result = pipeline.process_tool_result(event).await.unwrap();

        // Both hooks should have run
        let text = result.text_content();
        assert!(text.contains("PREFIX:"));
        assert!(text.contains(":SUFFIX"));
    }

    #[tokio::test]
    async fn test_pipeline_hook_error_handling() {
        let (pipeline, _temp) = setup_pipeline_with_plugin(
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "bad_handler" }] }
}

pub fn bad_handler(ctx, event) {
    // This will cause an error - accessing non-existent field
    let x = event.nonexistent.field;
    event
}
"#,
        )
        .await;

        let event = make_event("tool", "original");
        // Pipeline should handle error gracefully and return original event
        let result = pipeline.process_tool_result(event).await.unwrap();

        assert_eq!(result.text_content(), "original");
    }

    #[tokio::test]
    async fn test_pipeline_pattern_filtering() {
        let (pipeline, _temp) = setup_pipeline_with_plugin(
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "just_test*", handler: "filter" }] }
}

pub fn filter(ctx, event) {
    event.content = [#{ type: "text", text: "filtered!" }];
    event
}
"#,
        )
        .await;

        // Matching pattern
        let event1 = make_event("just_test_verbose", "original");
        let result1 = pipeline.process_tool_result(event1).await.unwrap();
        assert_eq!(result1.text_content(), "filtered!");

        // Non-matching pattern
        let event2 = make_event("just_build", "original");
        let result2 = pipeline.process_tool_result(event2).await.unwrap();
        assert_eq!(result2.text_content(), "original"); // unchanged
    }
}
