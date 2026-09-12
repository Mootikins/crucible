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
//!
//! # One writer, and it takes the whole set
//!
//! The daemon sends the session's complete expression set on every push, so a
//! key missing from one is a key the daemon RELEASED: a provider cleared it, or
//! the plugin that set it went inert. A per-key writer could only ever add, so
//! a released value stayed on the bar until the client exited — the same
//! failure as a surface nothing withdraws, arriving at the last hop. Hence
//! [`replace`] and nothing beside it.

use std::collections::BTreeMap;
use std::sync::RwLock;

/// Longest value a single expression may render.
///
/// A safety limit, not the layout mechanism: truncation for display is the
/// TUI's job, because only the TUI knows the terminal width, and character
/// count is not display width (CJK is two cells per char).
const MAX_VALUE_CHARS: usize = 256;

static EXPRS: RwLock<Option<BTreeMap<String, String>>> = RwLock::new(None);

/// Strip anything that could drive or reorder the terminal.
///
/// The daemon sanitises too; this is the same rule applied again on arrival,
/// because the client should not have to trust that it did. Control characters
/// move the cursor and emit OSC sequences; bidi overrides and zero-width
/// characters are not control characters but reorder how the bar *reads*.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            !(c.is_control()
                || matches!(c,
                    '\u{200E}' | '\u{200F}' | '\u{061C}'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{200B}'..='\u{200D}' | '\u{FEFF}'))
        })
        .take(MAX_VALUE_CHARS)
        .collect()
}

/// Install the daemon's whole set. Answers `true` when the rendered set
/// actually changed.
///
/// The dirty check matters because this crosses a socket: a provider firing on
/// every turn with an unchanged set should cost nothing, not a repaint.
pub fn replace(values: BTreeMap<String, String>) -> bool {
    let clean: BTreeMap<String, String> = values
        .into_iter()
        .map(|(key, value)| (key, sanitize(&value)))
        .collect();
    let Ok(mut guard) = EXPRS.write() else {
        return false;
    };
    // An untouched store draws nothing, so an empty set arriving at one is not
    // a change — the first full snapshot of a session with no expressions must
    // not cost a repaint.
    let changed = match guard.as_ref() {
        Some(existing) => *existing != clean,
        None => !clean.is_empty(),
    };
    if changed {
        *guard = Some(clean);
    }
    changed
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

    /// Apply a set the way the daemon's payload does.
    fn push(pairs: &[(&str, &str)]) -> bool {
        replace(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }

    #[test]
    fn a_pushed_value_is_readable() {
        assert!(push(&[("git", "main*")]));
        assert_eq!(snapshot().get("git").map(String::as_str), Some("main*"));
    }

    /// Push models generate redundant traffic; every comparable implementation
    /// grew a dirty check for this reason, and ours crosses a socket.
    #[test]
    fn re_pushing_an_unchanged_set_reports_no_change() {
        assert!(push(&[("k", "v")]));
        assert!(!push(&[("k", "v")]), "unchanged must not signal a repaint");
        assert!(push(&[("k", "w")]));
    }

    /// The value a released expression leaves behind must stop being DRAWN.
    /// The daemon pushes a full snapshot, so a key absent from it has to go —
    /// with a per-key writer the client kept painting a value nothing could
    /// ever refresh, for the rest of its life.
    #[test]
    fn a_key_the_daemon_no_longer_sends_stops_rendering() {
        assert!(push(&[("git", "main*"), ("oci", "sandboxed")]));
        assert_eq!(snapshot().len(), 2);

        assert!(push(&[("git", "main*")]), "dropping a key is a change");

        assert_eq!(snapshot().get("git").map(String::as_str), Some("main*"));
        assert!(
            !snapshot().contains_key("oci"),
            "a key the daemon released must stop rendering"
        );
    }

    /// The end state after the last value is released: reachable, and not read
    /// as "an empty payload, so keep what you had".
    #[test]
    fn an_empty_set_clears_every_value() {
        assert!(push(&[("git", "main")]));
        assert!(push(&[]));
        assert!(snapshot().is_empty());
    }

    /// A session that never set an expression must not cost a repaint on its
    /// first snapshot.
    #[test]
    fn an_empty_set_on_an_untouched_store_is_not_a_change() {
        assert!(!push(&[]));
    }

    /// The security property. A branch name or model-derived string must not be
    /// able to move the cursor or emit an OSC sequence.
    #[test]
    fn control_characters_are_stripped() {
        push(&[("evil", "main\x1b[2J\x1b]0;pwned\x07\r\n")]);
        let got = snapshot()["evil"].clone();
        assert!(!got.contains('\x1b'), "escape survived: {got:?}");
        assert!(!got.contains('\x07'), "bell survived: {got:?}");
        assert!(!got.contains('\n'), "newline survived: {got:?}");
        assert!(got.starts_with("main"), "legible text kept: {got:?}");
    }

    #[test]
    fn values_are_capped_as_a_safety_limit() {
        push(&[("long", &"x".repeat(MAX_VALUE_CHARS * 2))]);
        assert_eq!(snapshot()["long"].chars().count(), MAX_VALUE_CHARS);
    }

    /// Multibyte must be counted in characters, not bytes — slicing by byte
    /// offset would split a UTF-8 sequence.
    #[test]
    fn multibyte_values_are_not_split_mid_character() {
        push(&[("cjk", &"日".repeat(MAX_VALUE_CHARS * 2))]);
        let got = snapshot()["cjk"].clone();
        assert_eq!(got.chars().count(), MAX_VALUE_CHARS);
        assert!(got.chars().all(|c| c == '日'));
    }
}
