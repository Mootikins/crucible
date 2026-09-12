//! `cru.statusline.set` / `clear` — daemon-side statusline expression values.
//!
//! A bar places an expression with `sl.expr("git")`; a handler running anywhere
//! daemon-side supplies its value. The split exists because the value is a fact
//! only the daemon holds — the workspace, the shell, the kiln — while the layout
//! is a fact only the TUI can act on.
//!
//! Values are **structured text**, never pre-styled escape sequences. Shipping
//! resolved ANSI across a process boundary is a mistake with two heads: the
//! sender styles against a theme the receiver may not have, and the receiver
//! cannot sanitize the result without also stripping the styling. Keeping the
//! styling structural (`sl.expr("git"):hl("Git")`) lets the TUI escape control
//! characters unconditionally.
//!
//! Session-scoped, matching `cru.context.attach`. Rust owns the caps.
//!
//! # A value is released, never only overwritten
//!
//! Two things end a value besides the next push: the session ends
//! ([`StatuslineExprRegistry::release_session`]), and the plugin that set it
//! goes inert ([`StatuslineExprRegistry::release_source`]). The store had
//! neither, which cost two defects at once. A session's map outlived the
//! session for the daemon's life, and a plugin marked Not Active left its value
//! painted in every attached client with nothing that could ever refresh it.
//!
//! So each value records its [`LuaSource`]. A map keyed by session can only be
//! released by session unless something on the value names another axis, and
//! being keyed by session is not by itself a reason a store needs no
//! plugin-scoped release.

use crate::error::LuaError;
use crate::host_hook::HostHook;
use crate::plugin_context::LuaSource;
use mlua::{Lua, Table};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Longest value one expression may carry.
///
/// A safety limit, not the layout mechanism — truncation for display belongs to
/// the TUI, which is the only side that knows the terminal width, and character
/// count is not display width.
pub const MAX_VALUE_CHARS: usize = 256;

/// Most expressions one session may define. A statusline is one line; an
/// unbounded producer should not be able to crowd out the mode indicator.
pub const MAX_KEYS_PER_SESSION: usize = 16;

/// Why a value was not recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprRejection {
    /// The value is identical to the one already stored. Not an error — the
    /// dirty check exists so a handler firing every turn with an unchanged
    /// value costs nothing. Push models generate redundant traffic, and this
    /// one crosses a socket.
    Unchanged,
    /// This session already defines the maximum number of expressions.
    TooManyKeys {
        max: usize,
    },
    /// Empty key.
    EmptyKey,
    Unavailable,
}

impl ExprRejection {
    pub fn reason(&self) -> String {
        match self {
            Self::Unchanged => "value unchanged; no repaint needed".to_string(),
            Self::TooManyKeys { max } => format!("too many statusline expressions (max {max})"),
            Self::EmptyKey => "empty expression key".to_string(),
            Self::Unavailable => "statusline registry unavailable".to_string(),
        }
    }
}

/// Called when a value actually changes, so the host can tell clients to
/// repaint. Boxed rather than a concrete channel type: this crate must not
/// depend on the daemon's event bus.
pub type ChangeNotifier = Arc<dyn Fn(&str) + Send + Sync>;

/// One recorded value, and who recorded it.
///
/// The source is what makes a plugin-scoped release possible at all. The map is
/// keyed by session, so without an author on each value `make_plugin_inert` has
/// nothing to name: it would have to drop every session's whole bar, blanking
/// the user's own expressions along with the plugin's.
struct Expr {
    text: String,
    source: LuaSource,
}

/// Per-session expression values.
#[derive(Default)]
pub struct StatuslineExprRegistry {
    sessions: Mutex<HashMap<String, HashMap<String, Expr>>>,
    /// Installed once by the daemon at boot. See [`HostHook`].
    on_change: HostHook<ChangeNotifier>,
}

impl std::fmt::Debug for StatuslineExprRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatuslineExprRegistry")
            .field("notifies", &self.on_change.is_installed())
            .finish_non_exhaustive()
    }
}

/// Strip forbidden characters and cap the length.
pub fn sanitize(value: &str) -> String {
    sanitize_uncapped(value)
        .chars()
        .take(MAX_VALUE_CHARS)
        .collect()
}

/// Strip forbidden characters, leaving length to the caller.
///
/// Literal text in a bar is authored, not pushed, so it is not subject to the
/// value cap — but it is not necessarily *typed* by the author either. A config
/// that interpolates a branch name into a literal (`{ "on " .. branch }`) is
/// carrying the same attacker-influenced data an expression would, through a
/// variant that used to be trusted purely because of where it came from.
pub fn sanitize_uncapped(value: &str) -> String {
    crucible_core::text::sanitize_single_line(value)
}

impl StatuslineExprRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the change notifier, once. Answers `false` when one is already
    /// installed, which is a double boot rather than something to paper over.
    ///
    /// Writing a value and *telling a client about it* are different events —
    /// over a socket, with the TUI idle-blocked on input, a changed value does
    /// not repaint anything by itself. Every comparable statusline
    /// implementation needs an explicit "now redraw" signal for the same reason.
    #[must_use]
    pub fn set_change_notifier(&self, notifier: ChangeNotifier) -> bool {
        self.on_change.install(notifier)
    }

    fn notify(&self, session_id: &str) {
        // The sessions lock is already released here: host code must never run
        // under it, because the notifier reaches the daemon's event bus.
        if let Some(notify) = self.on_change.get() {
            notify(session_id);
        }
    }

    /// Record a value. `Ok(text)` when it changed and the TUI should repaint.
    ///
    /// `source` is who set it, so [`Self::release_source`] can take it back
    /// again. The host reads it from the VM's ambient source and a caller never
    /// names one — a plugin naming another source would make the release
    /// unreachable, which is the whole reason to record it.
    pub fn set(
        &self,
        session_id: &str,
        key: &str,
        value: &str,
        source: LuaSource,
    ) -> Result<String, ExprRejection> {
        if key.is_empty() {
            return Err(ExprRejection::EmptyKey);
        }
        let clean = sanitize(value);

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| ExprRejection::Unavailable)?;
        let entry = sessions.entry(session_id.to_string()).or_default();

        // The dirty check reads the text and not the source: a repaint is a
        // question about what a client draws. So the slot keeps the source that
        // last CHANGED it, and a second source pushing an identical string does
        // not take the slot over.
        if entry
            .get(key)
            .is_some_and(|existing| existing.text == clean)
        {
            return Err(ExprRejection::Unchanged);
        }
        if !entry.contains_key(key) && entry.len() >= MAX_KEYS_PER_SESSION {
            return Err(ExprRejection::TooManyKeys {
                max: MAX_KEYS_PER_SESSION,
            });
        }

        entry.insert(
            key.to_string(),
            Expr {
                text: clean.clone(),
                source,
            },
        );
        drop(sessions);
        self.notify(session_id);
        Ok(clean)
    }

    /// Drop a value so its item renders nothing again.
    pub fn clear(&self, session_id: &str, key: &str) -> bool {
        let removed = self
            .sessions
            .lock()
            .ok()
            .and_then(|mut s| s.get_mut(session_id).map(|e| e.remove(key).is_some()))
            .unwrap_or(false);
        if removed {
            self.notify(session_id);
        }
        removed
    }

    /// Current values for a session. Rides along in the `ui.config` snapshot so
    /// a TUI attaching after a value was set is not blank until the next push.
    pub fn snapshot(&self, session_id: &str) -> HashMap<String, String> {
        self.sessions
            .lock()
            .ok()
            .and_then(|s| {
                s.get(session_id).map(|entries| {
                    entries
                        .iter()
                        .map(|(key, expr)| (key.clone(), expr.text.clone()))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// Drop every value one source set, and tell each affected session to
    /// repaint. Answers how many values it dropped.
    ///
    /// For a source that goes inert — a plugin the daemon marks Not Active.
    /// `make_plugin_inert` promises "nothing of this plugin's is registered or
    /// running"; dropping its handlers stops the NEXT push, and only this stops
    /// the last one being painted in every attached client for the daemon's
    /// life. That is the same failure as a surface nothing withdraws.
    ///
    /// Not called on a reload that succeeds: the plugin runs again and refreshes
    /// its own values, so blanking the bar in between only flickers.
    pub fn release_source(&self, source: &LuaSource) -> usize {
        // Collected under the lock and announced after it, for the reason
        // `notify` gives: the notifier reaches the daemon's event bus.
        let mut dropped = 0usize;
        let affected: Vec<String> = {
            let Ok(mut sessions) = self.sessions.lock() else {
                return 0;
            };
            let mut affected = Vec::new();
            for (session_id, entries) in sessions.iter_mut() {
                let before = entries.len();
                entries.retain(|_, expr| expr.source != *source);
                if entries.len() < before {
                    dropped += before - entries.len();
                    affected.push(session_id.clone());
                }
            }
            sessions.retain(|_, entries| !entries.is_empty());
            affected
        };
        for session_id in &affected {
            self.notify(session_id);
        }
        dropped
    }

    /// Forget a session's values on its way out. Answers how many it dropped.
    ///
    /// The map is keyed by session and had no production release, so every
    /// session that ever set an expression kept its map for the daemon's life.
    ///
    /// No repaint: the session is ending, so there is no bar left to draw. The
    /// notifier would build a payload for a session whose subscribers are
    /// already going away.
    pub fn release_session(&self, session_id: &str) -> usize {
        self.sessions
            .lock()
            .ok()
            .and_then(|mut s| s.remove(session_id))
            .map_or(0, |entries| entries.len())
    }
}

/// Register `cru.statusline.set/clear` on the `cru` table.
///
/// The registry is passed in, never created here. VMs are built lazily *and*
/// cached, so a VM built before a late `OnceLock` bind would hold a nil function
/// permanently — and since this fails open, the value would simply never appear,
/// with nothing logged. Owning the registry eagerly upstream removes the race.
pub fn register_statusline_exprs(
    lua: &Lua,
    cru: &Table,
    registry: Arc<StatuslineExprRegistry>,
) -> Result<(), LuaError> {
    let statusline = match cru.get::<Table>("statusline") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            cru.set("statusline", t.clone())?;
            t
        }
    };

    let mut ns = crate::host_registry::Ns::over(lua, "cru.statusline", statusline);

    let set_registry = Arc::clone(&registry);
    ns.func(
        "set",
        "(session_id: string, key: string, value: string) -> \
         { ok: boolean, value: string?, reason: string?, unchanged: boolean? }",
        move |lua, (session_id, key, value): (String, String, String)| {
            let result = lua.create_table()?;
            // The AMBIENT source, never an argument: this is the tag
            // `release_source` releases by, so a caller free to name one could
            // park a value nothing releases.
            let source = crate::plugin_context::current_source(lua);
            match set_registry.set(&session_id, &key, &value, source) {
                Ok(text) => {
                    result.set("ok", true)?;
                    result.set("value", text)?;
                }
                Err(rejection) => {
                    result.set("ok", false)?;
                    result.set("reason", rejection.reason())?;
                    // An unchanged value is a normal outcome, not a failure —
                    // callers that repaint on `ok` should not repaint on it.
                    result.set("unchanged", rejection == ExprRejection::Unchanged)?;
                }
            }
            Ok(result)
        },
    )?;
    ns.doc(
        "set",
        "Fill the slot `cru.statusline.expr(key)` renders. Answers a result \
         table rather than raising. `ok = false` with `unchanged = true` means \
         the value was already that, which is a normal outcome — repaint on \
         `ok`, not on the absence of it.",
    );

    let clear_registry = Arc::clone(&registry);
    ns.func(
        "clear",
        "(session_id: string, key: string) -> boolean",
        move |_, (session_id, key): (String, String)| Ok(clear_registry.clear(&session_id, &key)),
    )?;
    ns.doc(
        "clear",
        "Empty one slot. Answers whether the key had a value; clearing a key \
         that was never set is not an error.",
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_recorded_and_readable() {
        let r = StatuslineExprRegistry::new();
        assert_eq!(
            r.set("s1", "git", "main*", LuaSource::UserLua),
            Ok("main*".to_string())
        );
        assert_eq!(
            r.snapshot("s1").get("git").map(String::as_str),
            Some("main*")
        );
    }

    /// The dirty check: a provider firing every turn with the same value must
    /// not cost a repaint, because this crosses a socket.
    #[test]
    fn an_unchanged_value_is_rejected_as_unchanged() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        assert_eq!(
            r.set("s1", "git", "main", LuaSource::UserLua),
            Err(ExprRejection::Unchanged)
        );
        assert!(r.set("s1", "git", "main*", LuaSource::UserLua).is_ok());
    }

    #[test]
    fn values_are_session_scoped() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        assert!(r.snapshot("s2").is_empty());
    }

    #[test]
    fn clearing_removes_the_value() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        assert!(r.clear("s1", "git"));
        assert!(!r.snapshot("s1").contains_key("git"));
        assert!(!r.clear("s1", "git"), "clearing twice is not a change");
    }

    /// A statusline is one line; a runaway producer must not crowd out the
    /// mode indicator.
    #[test]
    fn the_key_count_is_capped_per_session() {
        let r = StatuslineExprRegistry::new();
        for i in 0..MAX_KEYS_PER_SESSION {
            r.set("s1", &format!("k{i}"), "v", LuaSource::UserLua)
                .unwrap();
        }
        assert_eq!(
            r.set("s1", "one-too-many", "v", LuaSource::UserLua),
            Err(ExprRejection::TooManyKeys {
                max: MAX_KEYS_PER_SESSION
            })
        );
        // Updating an existing key still works at the cap.
        assert!(r.set("s1", "k0", "changed", LuaSource::UserLua).is_ok());
    }

    /// The security property: a value can originate in a branch name or model
    /// output, and must not be able to drive the terminal.
    #[test]
    fn control_characters_are_stripped() {
        let r = StatuslineExprRegistry::new();
        let stored = r
            .set(
                "s1",
                "evil",
                "main\x1b[2J\x1b]0;pwned\x07\r\n",
                LuaSource::UserLua,
            )
            .unwrap();
        assert!(!stored.contains('\x1b'), "escape survived: {stored:?}");
        assert!(!stored.contains('\x07'), "bell survived: {stored:?}");
        assert!(!stored.contains('\n'), "newline survived: {stored:?}");
        assert!(stored.starts_with("main"));
    }

    /// Bidi overrides are not control characters, so `is_control` alone misses
    /// them — and they reorder how the bar reads without changing what it
    /// contains. A branch name is attacker-influenced in any repo you clone.
    #[test]
    fn bidi_and_zero_width_characters_are_stripped() {
        let r = StatuslineExprRegistry::new();
        let stored = r
            .set(
                "s1",
                "git",
                "feat/\u{202E}txt.exe\u{200B}\u{2066}x",
                LuaSource::UserLua,
            )
            .unwrap();

        for bad in ['\u{202E}', '\u{200B}', '\u{2066}'] {
            assert!(
                !stored.contains(bad),
                "{bad:?} survived sanitising: {stored:?}"
            );
        }
        assert!(stored.starts_with("feat/"), "legible text kept: {stored:?}");
    }

    #[test]
    fn values_are_capped_by_character_not_byte() {
        let r = StatuslineExprRegistry::new();
        let stored = r
            .set(
                "s1",
                "cjk",
                &"日".repeat(MAX_VALUE_CHARS * 2),
                LuaSource::UserLua,
            )
            .unwrap();
        assert_eq!(stored.chars().count(), MAX_VALUE_CHARS);
        assert!(
            stored.chars().all(|c| c == '日'),
            "a byte-based cap would split a UTF-8 sequence"
        );
    }

    /// The notifier is installed once at boot. A second install used to replace
    /// the first in silence, which is how a double boot looked exactly like a
    /// working one.
    #[test]
    fn the_notifier_installs_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let r = StatuslineExprRegistry::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        assert!(r.set_change_notifier(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        })));
        assert!(
            !r.set_change_notifier(Arc::new(|_| panic!("the second notifier must never fire"))),
            "the second install is refused"
        );

        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "the first notifier survives"
        );
    }

    /// The dirty check is what makes the notifier cheap: an unchanged value
    /// must not fire it, or every turn would repaint every client.
    #[test]
    fn the_notifier_fires_only_on_a_real_change() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let r = StatuslineExprRegistry::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        assert!(r.set_change_notifier(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        })));

        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = r.set("s1", "git", "main", LuaSource::UserLua);
        assert_eq!(hits.load(Ordering::SeqCst), 1, "unchanged must not notify");

        r.set("s1", "git", "main*", LuaSource::UserLua).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);

        r.clear("s1", "git");
        assert_eq!(hits.load(Ordering::SeqCst), 3, "clearing is a change");

        r.clear("s1", "git");
        assert_eq!(hits.load(Ordering::SeqCst), 3, "clearing twice is not");
    }

    #[test]
    fn an_empty_key_is_rejected() {
        let r = StatuslineExprRegistry::new();
        assert_eq!(
            r.set("s1", "", "v", LuaSource::UserLua),
            Err(ExprRejection::EmptyKey)
        );
    }

    /// The leak this closes: the map is keyed by session and nothing released
    /// it, so every session that ever set an expression kept its map for the
    /// daemon's life.
    #[test]
    fn releasing_a_session_drops_its_values() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        r.set("s1", "kiln", "docs", LuaSource::Plugin("git".into()))
            .unwrap();
        r.set("s2", "git", "main", LuaSource::UserLua).unwrap();

        assert_eq!(r.release_session("s1"), 2);
        assert!(r.snapshot("s1").is_empty());
        assert_eq!(
            r.snapshot("s2").get("git").map(String::as_str),
            Some("main"),
            "another session is untouched"
        );
        assert_eq!(r.release_session("s1"), 0, "releasing twice drops nothing");
    }

    /// A plugin marked Not Active must leave nothing painted. Its handlers are
    /// gone, so nothing would ever overwrite the value it left behind.
    #[test]
    fn releasing_a_source_drops_only_that_sources_values() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "oci", "sandboxed", LuaSource::Plugin("oci".into()))
            .unwrap();
        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        r.set("s2", "oci", "sandboxed", LuaSource::Plugin("oci".into()))
            .unwrap();
        r.set("s2", "other", "x", LuaSource::Plugin("kanban".into()))
            .unwrap();

        assert_eq!(r.release_source(&LuaSource::Plugin("oci".into())), 2);

        assert!(!r.snapshot("s1").contains_key("oci"));
        assert_eq!(
            r.snapshot("s1").get("git").map(String::as_str),
            Some("main"),
            "the operator's own value survives a plugin going inert"
        );
        assert!(!r.snapshot("s2").contains_key("oci"));
        assert_eq!(
            r.snapshot("s2").get("other").map(String::as_str),
            Some("x"),
            "another plugin's value survives"
        );
    }

    /// The half that makes a client stop DRAWING it: a release nothing
    /// announces leaves the value painted, which is the surface-withdrawal
    /// failure one store over.
    #[test]
    fn releasing_a_source_notifies_every_affected_session() {
        let r = StatuslineExprRegistry::new();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        assert!(r.set_change_notifier(Arc::new(move |id: &str| {
            sink.lock().unwrap().push(id.to_string());
        })));

        r.set("s1", "oci", "on", LuaSource::Plugin("oci".into()))
            .unwrap();
        r.set("s2", "oci", "on", LuaSource::Plugin("oci".into()))
            .unwrap();
        r.set("s3", "git", "main", LuaSource::UserLua).unwrap();
        seen.lock().unwrap().clear();

        r.release_source(&LuaSource::Plugin("oci".into()));

        let mut notified = seen.lock().unwrap().clone();
        notified.sort();
        assert_eq!(
            notified,
            vec!["s1".to_string(), "s2".to_string()],
            "every session that lost a value must be told, and no other"
        );

        seen.lock().unwrap().clear();
        assert_eq!(r.release_source(&LuaSource::Plugin("absent".into())), 0);
        assert!(
            seen.lock().unwrap().is_empty(),
            "nothing dropped, nothing told"
        );
    }

    /// A session release happens on the session's way out, so there is no bar
    /// left to repaint and the notifier must not build a payload for it.
    #[test]
    fn releasing_a_session_does_not_notify() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let r = StatuslineExprRegistry::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        assert!(r.set_change_notifier(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        })));

        r.set("s1", "git", "main", LuaSource::UserLua).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        r.release_session("s1");
        assert_eq!(hits.load(Ordering::SeqCst), 1, "ending is not a repaint");
    }
}
