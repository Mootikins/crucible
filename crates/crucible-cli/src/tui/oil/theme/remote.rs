//! Applying the daemon's `ui.config` snapshot to this process's active theme.
//!
//! The TUI holds a complete compiled-in default and renders correctly without a
//! daemon. This snapshot is an **upgrade**, never a precondition for the first
//! frame: it may arrive late, fail to parse, or never arrive at all, and each of
//! those degrades to an unstyled-but-correct TUI rather than a blank screen.
//!
//! That ordering property is deliberate. Making theme availability a startup
//! precondition is a known crash source in comparable agents — an extension or
//! render path asking for a theme before one exists.

use serde_json::Value;
use tracing::{debug, warn};

/// Highest `ui.config` version this client understands.
const SUPPORTED_VERSION: u32 = 1;

/// Apply a `ui.config` payload to the process-wide active theme.
///
/// Returns `true` when a theme was installed. Returns `false` when the payload
/// carried nothing usable, or when the theme was already initialized — the
/// global is set-once, so a late second snapshot cannot silently retheme a
/// half-drawn screen.
pub fn apply_ui_config(payload: &Value) -> bool {
    let version = payload.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version > u64::from(SUPPORTED_VERSION) {
        // Forward compatible: take the fields we know, ignore the rest. A newer
        // daemon may describe surfaces this client cannot draw.
        debug!(
            version,
            supported = SUPPORTED_VERSION,
            "ui.config is newer than this client; applying the understood subset"
        );
    }

    let Some(theme_wire) = payload.get("theme") else {
        warn!("ui.config carried no theme; keeping the compiled-in default");
        return false;
    };

    let theme = crucible_lua::theme_wire::theme_from_wire(theme_wire);
    let name = theme.name.clone();

    if super::global::is_initialized() {
        debug!(theme = %name, "active theme already set; ignoring later ui.config");
        return false;
    }

    super::global::set(theme);
    debug!(theme = %name, "applied theme from ui.config");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // NOTE: these tests write the process-wide `ACTIVE_THEME` OnceLock. They are
    // correct under cargo-nextest, which runs each test in its own process (this
    // repo's runner). Under plain `cargo test` they would share one global and
    // interfere — which is why the assertions below live in separate tests
    // rather than one combined case.

    #[test]
    fn applying_a_wire_theme_sets_the_active_theme() {
        let payload = json!({
            "version": 1,
            "theme": { "name": "from-lua", "is_dark": true },
        });

        assert!(apply_ui_config(&payload));
        assert_eq!(super::super::active().name, "from-lua");
    }

    /// The whole point of the compiled-in default: no daemon, no snapshot, still
    /// a correct screen.
    #[test]
    fn without_a_snapshot_the_compiled_in_default_is_active() {
        let theme = super::super::active();
        assert_eq!(theme.name, "crucible-dark");
        assert!(theme.is_dark);
    }

    /// Fail to a complete theme, never a partial one. A payload missing colours
    /// must not leave holes — `bg` without `fg` is invisible text.
    #[test]
    fn a_payload_without_colors_yields_a_complete_theme() {
        let payload = json!({ "version": 1, "theme": { "name": "sparse" } });

        assert!(apply_ui_config(&payload));
        let active = super::super::active();
        assert_eq!(active.name, "sparse");
        assert_eq!(
            active.colors,
            crucible_lua::theme::ThemeConfig::default_dark().colors,
            "missing colours fall back wholesale, not per-field"
        );
    }

    #[test]
    fn a_payload_with_no_theme_is_ignored() {
        assert!(!apply_ui_config(&json!({ "version": 1 })));
    }

    /// A newer daemon must not brick an older TUI.
    #[test]
    fn a_future_version_still_applies_the_understood_subset() {
        let payload = json!({
            "version": 99,
            "theme": { "name": "from-the-future" },
            "surfaces_we_cannot_draw": { "hologram": {} },
        });

        assert!(apply_ui_config(&payload));
        assert_eq!(super::super::active().name, "from-the-future");
    }
}
