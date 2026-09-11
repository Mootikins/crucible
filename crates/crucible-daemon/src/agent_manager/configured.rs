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

/// The token budget for a session's assembled context.
///
/// Precedence: an explicit `chat.context_budget` wins, then the window the
/// provider reported for this session's model, then the shipped fallback.
///
/// This never answers `None`. It used to: `context_budget` defaulted to
/// `None`, and both `should_autocompact` and `enforce_context_budget` return
/// early on `None`, so auto-compaction and truncation were dead on every
/// default session. The daemon already discovered the real window at session
/// start and spent it on a display event.
pub(crate) fn context_budget(discovered: Option<usize>) -> usize {
    leaf("chat.context_budget")
        .as_ref()
        .and_then(serde_json::Value::as_u64)
        .map(|n| n as usize)
        .or(discovered)
        .unwrap_or(crucible_core::config::components::chat::DEFAULT_CONTEXT_BUDGET)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing configured and nothing discovered still yields a budget.
    ///
    /// This is the defect the derivation fixes. `context_budget` used to
    /// default to `None`, and both `should_autocompact` and
    /// `enforce_context_budget` return early without a budget — so on a
    /// default session neither auto-compaction nor truncation ever ran.
    #[test]
    fn an_unconfigured_session_still_gets_a_budget() {
        assert_eq!(
            context_budget(None),
            crucible_core::config::components::chat::DEFAULT_CONTEXT_BUDGET
        );
    }

    /// The window the provider reported beats the shipped fallback.
    #[test]
    fn a_discovered_window_beats_the_fallback() {
        assert_eq!(context_budget(Some(32_000)), 32_000);
    }

    /// An explicit key beats the discovered window: a user who pins a budget
    /// smaller than the model's window means it.
    #[test]
    fn an_explicit_key_beats_the_discovered_window() {
        crucible_lua::seed_app_config(serde_json::json!({
            "chat": { "context_budget": 8_000 }
        }));
        assert_eq!(context_budget(Some(200_000)), 8_000);
    }
}
