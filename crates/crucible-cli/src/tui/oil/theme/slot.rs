//! One swappable, leak-on-install store for a render-path value.
//!
//! The theme, highlight table, geometry and bars all need the same shape: a
//! value the daemon can re-send at runtime, read as `&'static T` so a render
//! borrows from it for free. See `global` for why an install leaks the
//! previous value instead of reference-counting it.

use std::sync::{OnceLock, RwLock};

pub struct RenderSlot<T: 'static> {
    active: RwLock<Option<&'static T>>,
    fallback: OnceLock<T>,
}

impl<T: 'static> Default for RenderSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> RenderSlot<T> {
    pub const fn new() -> Self {
        Self {
            active: RwLock::new(None),
            fallback: OnceLock::new(),
        }
    }

    /// Install a value, replacing any previous one.
    pub fn set(&self, value: T) {
        let leaked: &'static T = Box::leak(Box::new(value));
        if let Ok(mut guard) = self.active.write() {
            *guard = Some(leaked);
        }
    }

    /// The installed value, or `fallback` when none was installed.
    ///
    /// A read never installs the fallback. If it did, a render before the
    /// daemon's `ui.config` arrives would latch the fallback, and a later
    /// [`Self::set`] would look like a no-op with nothing logged.
    pub fn get(&'static self, fallback: fn() -> T) -> &'static T {
        self.active
            .read()
            .ok()
            .and_then(|g| *g)
            .unwrap_or_else(|| self.fallback.get_or_init(fallback))
    }
}
