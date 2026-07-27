//! Process-wide statusline bar definitions for the TUI.
//!
//! Unlike the theme, geometry and highlight table — which fall back to "unset"
//! so components keep their built-ins — bars fall back to a **complete built-in
//! bar**. A statusline with no items is a blank line, not an unstyled one, so
//! there is nothing sensible for a component to fall back to on its own.

use crucible_lua::statusline_items::{builtin_default, StatusBars};
use std::sync::{OnceLock, RwLock};

static BARS: RwLock<Option<&'static StatusBars>> = RwLock::new(None);
static FALLBACK: OnceLock<StatusBars> = OnceLock::new();

/// Install bar definitions, replacing any previous set.
pub fn set(bars: StatusBars) {
    let leaked: &'static StatusBars = Box::leak(Box::new(bars));
    if let Ok(mut guard) = BARS.write() {
        *guard = Some(leaked);
    }
}

/// Active bars; the built-in default when none was delivered.
///
/// Reading never initializes `BARS` — see the note in `geometry::active`.
pub fn active() -> &'static StatusBars {
    BARS.read()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(|| FALLBACK.get_or_init(builtin_default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::statusline_items::{Anchor, StatusBarDef, StatusItem};

    #[test]
    fn without_delivery_the_builtin_bar_is_active() {
        let bars = active();
        assert!(
            bars.contains_key("main"),
            "there must always be a bar to draw"
        );
        assert_eq!(bars["main"].anchor, Anchor::FooterBelowInput);
    }

    #[test]
    fn delivered_bars_replace_the_default() {
        let mut bars = StatusBars::new();
        bars.insert(
            "custom".to_string(),
            StatusBarDef {
                anchor: Anchor::Top,
                items: vec![StatusItem::Mode],
            },
        );
        set(bars);

        assert!(active().contains_key("custom"));
        assert!(
            !active().contains_key("main"),
            "a config that defines bars replaces the default rather than merging"
        );
    }
}
