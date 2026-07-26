//! Cache of daemon-pushed statusline expression values.
//!
//! Unlike the theme, geometry and bar definitions — all set once at startup —
//! this changes throughout a session, so it is a lock rather than a `OnceLock`.
//!
//! Values are **structured text**, never pre-styled escape sequences. A daemon
//! that could emit ANSI into a status value would put a branch name or a
//! model-derived string on a path that can carry cursor-movement and OSC
//! sequences, and the receiving side could not sanitize it without also
//! stripping the styling. Keeping styling structural (via highlight groups on
//! the item) means control characters can be escaped unconditionally.

use std::collections::BTreeMap;
use std::sync::RwLock;

/// Longest value a single expression may render.
///
/// A safety limit, not the layout mechanism: truncation for display is the
/// TUI's job, because only the TUI knows the terminal width, and character
/// count is not display width (CJK is two cells per char).
const MAX_VALUE_CHARS: usize = 256;

static EXPRS: RwLock<Option<BTreeMap<String, String>>> = RwLock::new(None);

/// Strip control characters, which must never reach the terminal from a value
/// computed elsewhere.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_VALUE_CHARS)
        .collect()
}

/// Record a pushed value. Returns `true` when it actually changed.
///
/// The dirty check matters because this crosses a socket: a provider firing on
/// every turn with an unchanged value should cost nothing, not a repaint.
pub fn set(key: &str, value: &str) -> bool {
    let clean = sanitize(value);
    let Ok(mut guard) = EXPRS.write() else {
        return false;
    };
    let map = guard.get_or_insert_with(BTreeMap::new);
    match map.get(key) {
        Some(existing) if *existing == clean => false,
        _ => {
            map.insert(key.to_string(), clean);
            true
        }
    }
}

/// Drop a value, so its item renders nothing again.
pub fn clear(key: &str) -> bool {
    let Ok(mut guard) = EXPRS.write() else {
        return false;
    };
    guard
        .as_mut()
        .map(|m| m.remove(key).is_some())
        .unwrap_or(false)
}

/// Snapshot for one frame's render.
pub fn snapshot() -> BTreeMap<String, String> {
    EXPRS
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pushed_value_is_readable() {
        assert!(set("git", "main*"));
        assert_eq!(snapshot().get("git").map(String::as_str), Some("main*"));
    }

    /// Push models generate redundant traffic; every comparable implementation
    /// grew a dirty check for this reason, and ours crosses a socket.
    #[test]
    fn re_pushing_an_unchanged_value_reports_no_change() {
        assert!(set("k", "v"));
        assert!(!set("k", "v"), "unchanged value must not signal a repaint");
        assert!(set("k", "w"));
    }

    #[test]
    fn clearing_removes_the_value() {
        set("k", "v");
        assert!(clear("k"));
        assert!(!snapshot().contains_key("k"));
        assert!(!clear("k"), "clearing twice is not a change");
    }

    /// The security property. A branch name or model-derived string must not be
    /// able to move the cursor or emit an OSC sequence.
    #[test]
    fn control_characters_are_stripped() {
        set("evil", "main\x1b[2J\x1b]0;pwned\x07\r\n");
        let got = snapshot()["evil"].clone();
        assert!(!got.contains('\x1b'), "escape survived: {got:?}");
        assert!(!got.contains('\x07'), "bell survived: {got:?}");
        assert!(!got.contains('\n'), "newline survived: {got:?}");
        assert!(got.starts_with("main"), "legible text kept: {got:?}");
    }

    #[test]
    fn values_are_capped_as_a_safety_limit() {
        set("long", &"x".repeat(MAX_VALUE_CHARS * 2));
        assert_eq!(snapshot()["long"].chars().count(), MAX_VALUE_CHARS);
    }

    /// Multibyte must be counted in characters, not bytes — slicing by byte
    /// offset would split a UTF-8 sequence.
    #[test]
    fn multibyte_values_are_not_split_mid_character() {
        set("cjk", &"日".repeat(MAX_VALUE_CHARS * 2));
        let got = snapshot()["cjk"].clone();
        assert_eq!(got.chars().count(), MAX_VALUE_CHARS);
        assert!(got.chars().all(|c| c == '日'));
    }
}
