//! Process-wide per-surface geometry for the TUI.
//!
//! Every field is optional, and `None` means the renderer keeps its built-in.
//! Callers therefore read this as an override, never as the source of truth —
//! which is what keeps an untouched surface untouched rather than blanked.

use crucible_lua::ui_geometry::UiGeometry;
use std::sync::{OnceLock, RwLock};

static GEOMETRY: RwLock<Option<&'static UiGeometry>> = RwLock::new(None);
static FALLBACK: OnceLock<UiGeometry> = OnceLock::new();

/// Install geometry, replacing any previous set. Swappable so a re-sent
/// `ui.config` takes effect without a restart; see `theme::global` for why the
/// previous value is leaked rather than reference-counted.
pub fn set(geometry: UiGeometry) {
    let leaked: &'static UiGeometry = Box::leak(Box::new(geometry));
    if let Ok(mut guard) = GEOMETRY.write() {
        *guard = Some(leaked);
    }
}

/// Active geometry; all-default when none was delivered.
///
/// Reading must never initialize `GEOMETRY` — `get_or_init` here would mean any
/// render before the daemon fetch permanently locks in the default and every
/// later `set` silently no-ops, so the whole feature would do nothing with no
/// error anywhere. The fallback lives in its own cell for that reason.
pub fn active() -> &'static UiGeometry {
    GEOMETRY
        .read()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(|| FALLBACK.get_or_init(UiGeometry::default))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::style::Border;

    #[test]
    fn without_delivery_every_surface_is_unset_so_built_ins_win() {
        let g = active();
        assert_eq!(g.popup.border, None);
        assert_eq!(g.popup.padding, None);
        assert_eq!(g.prompt.normal, None);
    }

    /// The point of the whole phase: a themed glyph reaches the component that
    /// draws it. `InputMode::prompt()` returned a compiled-in constant before.
    #[test]
    fn a_themed_prompt_glyph_reaches_the_input_component() {
        use crate::tui::oil::components::InputMode;

        assert_eq!(InputMode::Normal.prompt(), " > ", "built-in before theming");

        let mut g = UiGeometry::default();
        g.prompt.normal = Some("\u{276f} ".to_string());
        g.prompt.shell = Some("$ ".to_string());
        set(g);

        assert_eq!(InputMode::Normal.prompt(), "\u{276f} ");
        assert_eq!(InputMode::Shell.prompt(), "$ ");
        assert_eq!(
            InputMode::Command.prompt(),
            " : ",
            "a mode the theme did not set keeps its built-in glyph"
        );
    }

    #[test]
    fn delivered_geometry_becomes_active() {
        let mut g = UiGeometry::default();
        g.popup.border = Some(Border::Rounded);
        g.prompt.normal = Some("> ".to_string());
        set(g);

        assert_eq!(active().popup.border, Some(Border::Rounded));
        assert_eq!(active().prompt.normal.as_deref(), Some("> "));
        assert_eq!(
            active().modal.border,
            None,
            "a surface the theme did not mention keeps its built-in"
        );
    }
}
