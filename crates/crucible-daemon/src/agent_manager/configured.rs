//! The config-store leaves the daemon reads at session and turn boundaries.
//!
//! These were session knobs. Each one is a tuning value with one sensible
//! answer per install, not a decision a session makes for itself, so the store
//! owns them and `settings.json`, `:set` and the settings pane all reach them
//! the way they reach every other key.
//!
//! Read live rather than snapshotted at boot: a `config.set` RPC or a
//! settings-UI save must reach the next session without a daemon restart. The
//! compiled-in fallback below only runs before the store is seeded — a test
//! that builds a VM directly rather than booting a daemon.

use crucible_core::config::components::chat::ChatConfig;

/// One leaf of the live app config, if the store holds it.
fn leaf(path: &str) -> Option<serde_json::Value> {
    let config = crucible_lua::get_app_config()?;
    crucible_core::config::leaf_at(&config, path).cloned()
}

/// `chat.system_prompt` — what a session starts with when its agent card
/// names no prompt of its own.
pub(crate) fn system_prompt() -> Option<String> {
    let configured = leaf("chat.system_prompt")
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Some(configured.unwrap_or_else(|| ChatConfig::default().system_prompt))
}

/// `chat.precognition_results` — how many notes the pre-turn search injects.
pub(crate) fn precognition_results() -> usize {
    leaf("chat.precognition_results")
        .as_ref()
        .and_then(serde_json::Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or_else(|| ChatConfig::default().precognition_results)
}

/// `chat.autocompact_threshold` — the fraction of the context budget that
/// triggers a compaction. `0.0` disables it.
pub(crate) fn autocompact_threshold() -> f32 {
    leaf("chat.autocompact_threshold")
        .as_ref()
        .and_then(serde_json::Value::as_f64)
        .map(|f| f as f32)
        .unwrap_or_else(|| ChatConfig::default().autocompact_threshold)
}
