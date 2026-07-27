//! Process-wide statusline bar definitions for the TUI.
//!
//! Unlike the theme, geometry and highlight table — which fall back to "unset"
//! so components keep their built-ins — bars fall back to a **complete built-in
//! bar**. A statusline with no items is a blank line, not an unstyled one, so
//! there is nothing sensible for a component to fall back to on its own.

use crucible_lua::statusline_items::{builtin_default, Layout};
use std::sync::{OnceLock, RwLock};

static LAYOUT: RwLock<Option<&'static Layout>> = RwLock::new(None);
static FALLBACK: OnceLock<Layout> = OnceLock::new();

/// Install bar definitions, replacing any previous set.
pub fn set(layout: Layout) {
    let leaked: &'static Layout = Box::leak(Box::new(layout));
    if let Ok(mut guard) = LAYOUT.write() {
        *guard = Some(leaked);
    }
}

/// Active bars; the built-in default when none was delivered.
///
/// Reading never initializes `LAYOUT` — see the note in `geometry::active`.
pub fn active() -> &'static Layout {
    LAYOUT
        .read()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(|| FALLBACK.get_or_init(builtin_default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::statusline_items::{Element, StatusItem};

    #[test]
    fn without_delivery_the_builtin_layout_is_active() {
        let layout = active();
        assert!(
            layout.prompt.contains(&Element::Input),
            "there must always be somewhere to type"
        );
        assert!(
            layout.prompt.len() > 1,
            "the built-in places a status row alongside the input"
        );
    }

    #[test]
    fn a_delivered_layout_replaces_the_default() {
        set(Layout {
            top: vec![Element::Row(vec![StatusItem::Mode])],
            prompt: vec![Element::Input],
            bottom: vec![],
        });

        assert_eq!(active().top, vec![Element::Row(vec![StatusItem::Mode])]);
        assert_eq!(
            active().prompt,
            vec![Element::Input],
            "a delivered layout replaces the default rather than merging into it"
        );
    }
}
