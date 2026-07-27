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

use crate::error::LuaError;
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

/// Per-session expression values.
#[derive(Default)]
pub struct StatuslineExprRegistry {
    sessions: Mutex<HashMap<String, HashMap<String, String>>>,
    on_change: Mutex<Option<ChangeNotifier>>,
}

impl std::fmt::Debug for StatuslineExprRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatuslineExprRegistry")
            .finish_non_exhaustive()
    }
}

/// Whether a character may not appear in a statusline value.
///
/// Control characters are the obvious half: a value can originate in a branch
/// name, a shell command's output, or model-derived text, and none of those may
/// move the cursor or emit an OSC sequence.
///
/// The bidi and zero-width formatting characters are the less obvious half.
/// They are not control characters, so `is_control` misses them, but a
/// right-to-left override in a branch name reorders how the rest of the bar
/// *reads* without changing what it contains — the display-spoofing trick
/// behind filenames like `annexe\u{202E}cod.exe`. A status bar has no use for
/// them, so refusing them outright costs nothing.
pub fn is_forbidden(c: char) -> bool {
    c.is_control()
        || matches!(c,
            // LRM, RLM, ALM
            '\u{200E}' | '\u{200F}' | '\u{061C}'
            // embeddings and overrides: LRE, RLE, PDF, LRO, RLO
            | '\u{202A}'..='\u{202E}'
            // isolates: LRI, RLI, FSI, PDI
            | '\u{2066}'..='\u{2069}'
            // zero-width space / non-joiner / joiner / no-break space
            | '\u{200B}'..='\u{200D}' | '\u{FEFF}')
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
    value.chars().filter(|c| !is_forbidden(*c)).collect()
}

impl StatuslineExprRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install the change notifier.
    ///
    /// Writing a value and *telling a client about it* are different events —
    /// over a socket, with the TUI idle-blocked on input, a changed value does
    /// not repaint anything by itself. Every comparable statusline
    /// implementation needs an explicit "now redraw" signal for the same reason.
    pub fn set_change_notifier(&self, notifier: ChangeNotifier) {
        if let Ok(mut guard) = self.on_change.lock() {
            *guard = Some(notifier);
        }
    }

    fn notify(&self, session_id: &str) {
        // Cloned out before calling so the lock is not held across host code.
        let notifier = self.on_change.lock().ok().and_then(|g| g.clone());
        if let Some(notify) = notifier {
            notify(session_id);
        }
    }

    /// Record a value. `Ok(text)` when it changed and the TUI should repaint.
    pub fn set(&self, session_id: &str, key: &str, value: &str) -> Result<String, ExprRejection> {
        if key.is_empty() {
            return Err(ExprRejection::EmptyKey);
        }
        let clean = sanitize(value);

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| ExprRejection::Unavailable)?;
        let entry = sessions.entry(session_id.to_string()).or_default();

        if entry.get(key).is_some_and(|existing| *existing == clean) {
            return Err(ExprRejection::Unchanged);
        }
        if !entry.contains_key(key) && entry.len() >= MAX_KEYS_PER_SESSION {
            return Err(ExprRejection::TooManyKeys {
                max: MAX_KEYS_PER_SESSION,
            });
        }

        entry.insert(key.to_string(), clean.clone());
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
            .and_then(|s| s.get(session_id).cloned())
            .unwrap_or_default()
    }

    /// Forget a session's values.
    pub fn forget(&self, session_id: &str) {
        if let Ok(mut s) = self.sessions.lock() {
            s.remove(session_id);
        }
    }
}

/// Register `cru.statusline.set/clear` on the `cru` table.
///
/// The registry is passed in, never created here. Session VMs are lazy *and*
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

    let set_registry = Arc::clone(&registry);
    let set_fn = lua.create_function(
        move |lua, (session_id, key, value): (String, String, String)| {
            let result = lua.create_table()?;
            match set_registry.set(&session_id, &key, &value) {
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
    statusline.set("set", set_fn)?;

    let clear_registry = Arc::clone(&registry);
    let clear_fn = lua.create_function(move |_, (session_id, key): (String, String)| {
        Ok(clear_registry.clear(&session_id, &key))
    })?;
    statusline.set("clear", clear_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_recorded_and_readable() {
        let r = StatuslineExprRegistry::new();
        assert_eq!(r.set("s1", "git", "main*"), Ok("main*".to_string()));
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
        r.set("s1", "git", "main").unwrap();
        assert_eq!(r.set("s1", "git", "main"), Err(ExprRejection::Unchanged));
        assert!(r.set("s1", "git", "main*").is_ok());
    }

    #[test]
    fn values_are_session_scoped() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main").unwrap();
        assert!(r.snapshot("s2").is_empty());
    }

    #[test]
    fn clearing_removes_the_value() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main").unwrap();
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
            r.set("s1", &format!("k{i}"), "v").unwrap();
        }
        assert_eq!(
            r.set("s1", "one-too-many", "v"),
            Err(ExprRejection::TooManyKeys {
                max: MAX_KEYS_PER_SESSION
            })
        );
        // Updating an existing key still works at the cap.
        assert!(r.set("s1", "k0", "changed").is_ok());
    }

    /// The security property: a value can originate in a branch name or model
    /// output, and must not be able to drive the terminal.
    #[test]
    fn control_characters_are_stripped() {
        let r = StatuslineExprRegistry::new();
        let stored = r
            .set("s1", "evil", "main\x1b[2J\x1b]0;pwned\x07\r\n")
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
            .set("s1", "git", "feat/\u{202E}txt.exe\u{200B}\u{2066}x")
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
            .set("s1", "cjk", &"日".repeat(MAX_VALUE_CHARS * 2))
            .unwrap();
        assert_eq!(stored.chars().count(), MAX_VALUE_CHARS);
        assert!(
            stored.chars().all(|c| c == '日'),
            "a byte-based cap would split a UTF-8 sequence"
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
        r.set_change_notifier(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        }));

        r.set("s1", "git", "main").unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = r.set("s1", "git", "main");
        assert_eq!(hits.load(Ordering::SeqCst), 1, "unchanged must not notify");

        r.set("s1", "git", "main*").unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);

        r.clear("s1", "git");
        assert_eq!(hits.load(Ordering::SeqCst), 3, "clearing is a change");

        r.clear("s1", "git");
        assert_eq!(hits.load(Ordering::SeqCst), 3, "clearing twice is not");
    }

    #[test]
    fn an_empty_key_is_rejected() {
        let r = StatuslineExprRegistry::new();
        assert_eq!(r.set("s1", "", "v"), Err(ExprRejection::EmptyKey));
    }

    #[test]
    fn forgetting_a_session_drops_its_values() {
        let r = StatuslineExprRegistry::new();
        r.set("s1", "git", "main").unwrap();
        r.forget("s1");
        assert!(r.snapshot("s1").is_empty());
    }
}
