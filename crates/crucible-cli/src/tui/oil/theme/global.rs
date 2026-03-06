//! Global theme store for the TUI.
//!
//! Provides a process-wide [`ThemeConfig`] via [`OnceLock`], initialized lazily
//! with [`ThemeConfig::default_dark()`] on first access.

use std::sync::OnceLock;

use super::config::ThemeConfig;

static ACTIVE_THEME: OnceLock<ThemeConfig> = OnceLock::new();

/// Returns the active theme configuration.
///
/// Initializes with [`ThemeConfig::default_dark()`] on first call if
/// [`set`] was not called beforehand.
pub fn active() -> &'static ThemeConfig {
    ACTIVE_THEME.get_or_init(ThemeConfig::default_dark)
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

    #[test]
    fn theme_global_is_initialized_after_active() {
        let _ = active();
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
