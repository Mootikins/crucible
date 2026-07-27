//! Global theme store for the TUI.
//!
//! Swappable, not set-once. The daemon re-sends `ui.config` when a config is
//! re-evaluated or a theme is switched at runtime, and a client that could only
//! be themed once would need restarting to see it.
//!
//! Installing leaks the previous theme rather than reference-counting it. That
//! is deliberate: `active()` returning `&'static ThemeConfig` is what lets a
//! render borrow from it for free — including `InputStyle::prompt`, which hands
//! back a `&'static str` straight out of the store — and lets `ViewContext` stay
//! `Copy`. An `Arc` would push a lifetime or a clone into every one of ~60 call
//! sites on the render path. Theme installs are user-initiated and rare (a few
//! per session at most), each leaking a few KB, so the trade is a bounded, tiny
//! leak for a much simpler render path.

use std::sync::{OnceLock, RwLock};

use super::config::ThemeConfig;

static ACTIVE_THEME: RwLock<Option<&'static ThemeConfig>> = RwLock::new(None);
static FALLBACK_THEME: OnceLock<ThemeConfig> = OnceLock::new();

fn fallback() -> &'static ThemeConfig {
    FALLBACK_THEME.get_or_init(ThemeConfig::default_dark)
}

/// The active theme, or the built-in dark theme when none has been installed.
///
/// Reading deliberately does NOT install the default. If it did, any render or
/// probe before the daemon's `ui.config` arrives would latch the fallback and
/// make a later [`set`] look like a no-op — the theme would never apply, with
/// nothing logged and nothing to catch it.
pub fn active() -> &'static ThemeConfig {
    ACTIVE_THEME
        .read()
        .ok()
        .and_then(|g| *g)
        .unwrap_or_else(fallback)
}

/// Install a theme, replacing any previous one.
pub fn set(config: ThemeConfig) {
    let leaked: &'static ThemeConfig = Box::leak(Box::new(config));
    if let Ok(mut guard) = ACTIVE_THEME.write() {
        *guard = Some(leaked);
    }
}

/// Whether a theme has been installed. A read alone does not count.
pub fn is_initialized() -> bool {
    ACTIVE_THEME.read().is_ok_and(|g| g.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_global_active_returns_default_dark() {
        let theme = active();
        assert_eq!(theme.name, "crucible-dark");
        assert!(theme.is_dark);
    }

    #[test]
    fn theme_global_active_is_same_value() {
        assert_eq!(active().name, active().name);
    }

    /// The property the push stream needs: a second install replaces the first.
    #[test]
    fn a_later_theme_replaces_an_earlier_one() {
        let mut first = ThemeConfig::default_dark();
        first.name = "first".to_string();
        set(first);
        assert_eq!(active().name, "first");

        let mut second = ThemeConfig::default_dark();
        second.name = "second".to_string();
        set(second);
        assert_eq!(
            active().name,
            "second",
            "themes must be swappable at runtime"
        );
    }

    /// `is_initialized` means "a theme was installed", not "a theme was read".
    #[test]
    fn reading_the_theme_does_not_count_as_installing_one() {
        let _ = active();
        assert!(
            !is_initialized(),
            "a read must leave the slot open for a later set"
        );
    }

    #[test]
    fn installing_a_theme_marks_it_initialized() {
        set(ThemeConfig::default_dark());
        assert!(is_initialized());
    }

    #[test]
    fn theme_global_active_from_multiple_threads() {
        use std::thread;

        let handles: Vec<_> = (0..4)
            .map(|_| thread::spawn(|| active().name.clone()))
            .collect();

        let names: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(names.windows(2).all(|w| w[0] == w[1]));
    }
}
