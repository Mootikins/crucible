//! Global theme store for the TUI.
//!
//! Provides a process-wide [`ThemeConfig`] via [`OnceLock`], initialized lazily
//! with [`ThemeConfig::default_dark()`] on first access.

use std::sync::OnceLock;

use super::config::ThemeConfig;

static ACTIVE_THEME: OnceLock<ThemeConfig> = OnceLock::new();
static FALLBACK_THEME: OnceLock<ThemeConfig> = OnceLock::new();

// NOTE: ACTIVE_THEME uses OnceLock and cannot be safely reset between tests.
// OnceLock only allows initialization once per process — the first call to set() or
// get_or_init() wins, and subsequent calls are no-ops. This means tests that run in
// parallel or sequentially in the same process will see the theme set by the first test.
// Tests requiring specific themes should use a test-local theme mechanism (e.g., passing
// ThemeConfig as a parameter) rather than relying on the global singleton.

/// Returns the active theme configuration, or the built-in dark theme when none
/// has been installed.
///
/// Reading deliberately does NOT initialize `ACTIVE_THEME`. If it did, any
/// render or probe before the daemon's `ui.config` arrives would lock in the
/// default and make every later [`set`] a silent no-op — the theme would never
/// apply, with nothing logged and nothing to catch it.
pub fn active() -> &'static ThemeConfig {
    ACTIVE_THEME
        .get()
        .unwrap_or_else(|| FALLBACK_THEME.get_or_init(ThemeConfig::default_dark))
}

/// Initialize the global theme. Intended to be called once at startup.
///
/// If the theme is already initialized (by a prior `set()` or `active()` call),
/// this is a no-op — the original theme is preserved.
pub fn set(config: ThemeConfig) {
    let _ = ACTIVE_THEME.set(config);
}

/// Returns `true` if the global theme has been initialized.
pub fn is_initialized() -> bool {
    ACTIVE_THEME.get().is_some()
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
    fn theme_global_active_is_same_reference() {
        let t1 = active();
        let t2 = active();
        assert!(std::ptr::eq(t1, t2));
    }

    /// `is_initialized` means "a theme was installed", not "a theme was read".
    ///
    /// This previously asserted the opposite, which encoded a real bug: reading
    /// the theme marked it initialized, so a render or probe before the daemon's
    /// `ui.config` arrived would both lock in the default AND make
    /// `apply_ui_config` skip installing the real one — the theme silently never
    /// applied, from either side.
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
            .map(|_| {
                thread::spawn(|| {
                    let t = active();
                    std::ptr::addr_of!(*t) as usize
                })
            })
            .collect();

        let addrs: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        // All threads got the same &'static reference
        assert!(addrs.windows(2).all(|w| w[0] == w[1]));
    }
}
