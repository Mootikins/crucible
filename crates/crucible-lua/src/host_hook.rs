//! One storage shape for a host handle a lower crate installs once.
//!
//! Three registries in this crate must tell the daemon that something changed:
//! the statusline expressions, the publications and the surfaces. None of them
//! may depend on the daemon, so each holds a boxed closure the daemon installs
//! at boot. That is the **crate-dependency firewall** — the exemption
//! `AGENTS.md` grants to its enums-over-traits rule — and it is legitimate.
//!
//! Three *storage shapes* for it were not. Two sat in `Mutex<Option<_>>` and
//! one in `OnceLock`, so two of them permitted a silent second install and paid
//! for a lock plus an `Arc` clone on every write.
//!
//! # The shape, not the census
//!
//! This doc used to say "three registries", and the count was the whole
//! description. Two more slots of exactly this shape were therefore invisible
//! until someone counted again: [`crate::session_api::Session`]'s config RPC
//! and the plugin loader's `DaemonSessionApi`. Both are now here. A stated
//! count is a completeness claim, and this one had none to make, so the rule is
//! the shape rather than the list: **a host handle a lower crate holds, that
//! boot installs exactly once and every later caller only reads, belongs in
//! this type.** What is held is not always a callback — a `dyn` bridge object
//! is the same slot with the same lifetime.
//!
//! The one slot of this shape that stays out is the runtimepath extender
//! ([`crate::config::set_runtimepath_extender`]). It is installed for the boot
//! evaluation and set back to `None` when that ends, and a `OnceLock` cannot
//! serve a slot that has to empty again.
//!
//! No production path re-installs any of these: the plugin loader is built once
//! and never replaced, a plugin reload keeps the same registry instance, and a
//! `Session` handle is built fresh per fire site and bound once. So all of them
//! want install-once, which is what this type gives them. A second install
//! answers `false` rather than replacing the first — a double install is a
//! double boot, not something to paper over.
//!
//! The slot is shared through an `Arc`, so a registry that derives `Clone`
//! keeps one handle across every clone. A handle stored per clone would never
//! serve the reader that matters: the closure Lua captured holds its own
//! clone.

use std::sync::{Arc, OnceLock};

/// A host handle, installed once and read without a lock.
///
/// `T` is whatever the holder declares — an `Arc<dyn Fn(..) + Send + Sync>` for
/// a change hook, a boxed or `Arc`'d bridge object for a handle. This type says
/// nothing about the shape: the hooks take different arguments, and a shared
/// payload type would force owned structs and a one-field wrapper that exists
/// only for symmetry.
pub struct HostHook<T> {
    slot: Arc<OnceLock<T>>,
}

impl<T> HostHook<T> {
    /// An empty slot. A registry with no hook installed simply stores, which is
    /// what every test and the `cru plugin check` path want.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: Arc::new(OnceLock::new()),
        }
    }

    /// Install the hook. Answers `false` when one is already installed.
    ///
    /// Takes `&self` because every clone shares the slot.
    ///
    /// `#[must_use]`: a caller that drops the answer cannot tell a first
    /// install from a second, which is the silent double-install this type
    /// exists to make impossible.
    #[must_use]
    pub fn install(&self, hook: T) -> bool {
        self.slot.set(hook).is_ok()
    }

    /// The installed hook, or `None`.
    ///
    /// Borrows rather than clones: the caller fires the hook through this
    /// reference, so no `Arc` traffic happens on a write path.
    pub fn get(&self) -> Option<&T> {
        self.slot.get()
    }

    /// Whether a hook is installed. The thing worth reporting from `Debug`
    /// when a client is not redrawing.
    #[must_use]
    pub fn is_installed(&self) -> bool {
        self.slot.get().is_some()
    }
}

impl<T> Default for HostHook<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Hand-written: every clone must share the slot, and `T` is a closure with no
/// `Clone` of its own, so a derive would demand a bound the callbacks cannot
/// meet.
impl<T> Clone for HostHook<T> {
    fn clone(&self) -> Self {
        Self {
            slot: Arc::clone(&self.slot),
        }
    }
}

/// Hand-written for the same reason: a closure has no `Debug`.
impl<T> std::fmt::Debug for HostHook<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostHook")
            .field("installed", &self.is_installed())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type Counter = Arc<dyn Fn() + Send + Sync>;

    #[test]
    fn an_empty_hook_reports_nothing_installed() {
        let hook: HostHook<Counter> = HostHook::new();
        assert!(!hook.is_installed());
        assert!(hook.get().is_none());
    }

    /// A second install is a double boot. The first hook stays.
    #[test]
    fn the_second_install_is_refused() {
        let seen: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let hook: HostHook<Counter> = HostHook::new();

        let first = Arc::clone(&seen);
        assert!(hook.install(Arc::new(move || first.lock().unwrap().push(1))));
        let second = Arc::clone(&seen);
        assert!(
            !hook.install(Arc::new(move || second.lock().unwrap().push(2))),
            "the second install must be refused"
        );

        (hook.get().expect("installed"))();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![1],
            "the first hook must survive the refused second"
        );
    }

    /// The registries that hold one of these derive `Clone`, and the writer
    /// that matters is a clone Lua captured. A hook stored per clone would
    /// never fire for it.
    #[test]
    fn a_clone_shares_the_slot() {
        let hits: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let hook: HostHook<Counter> = HostHook::new();
        let clone = hook.clone();

        let sink = Arc::clone(&hits);
        assert!(hook.install(Arc::new(move || *sink.lock().unwrap() += 1)));

        assert!(clone.is_installed(), "the clone must see the install");
        (clone.get().expect("installed"))();
        assert_eq!(*hits.lock().unwrap(), 1);
        assert!(
            !clone.install(Arc::new(|| {})),
            "the clone must refuse a second install too"
        );
    }
}
