//! The validated kiln name — the registry key, and the only spelling of a kiln
//! that crosses the wire.
//!
//! A kiln used to be named by its path, which meant every `session.get`, every
//! Lua payload, every persisted transcript and the agent's own prompt carried
//! the user's directory layout — and that a caller could *invent* a kiln by
//! naming a directory. A name is neither: it selects an entry the user already
//! put in their config, and it selects nothing at all when there is no such
//! entry.
//!
//! Modelled on [`SessionId`](crate::session::SessionId), with three deliberate
//! divergences from its charset:
//!
//! - **A space is a character.** `Crucible Help` is a name a person chose, and
//!   the registry is the one place that name is written down. A charset that
//!   refused the space made every picker show a label nobody typed.
//! - **Case is kept, and ignored.** The spelling the user registered is the
//!   spelling every renderer shows; resolution folds ASCII case, so `Work` and
//!   `work` are one kiln rather than two. [`KilnName`] holds both: the display
//!   text, and the folded key that equality, hashing and ordering read.
//! - **64 bytes, not 128.** A session id is a filesystem component and needs the
//!   headroom; a name is a map key and a prompt line.
//!
//! Deliberately **not** `AsRef<Path>`: a name is not a path, and the whole
//! point of the type is that no caller can turn one into a directory without
//! going through the registry.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::str::FromStr;

/// The longest name we will accept. Well past any name a human types, short
/// enough that a name is never the reason a log line or a prompt wraps.
const MAX_LEN: usize = 64;

impl KilnName {
    /// The length limit, published because a caller deriving a name (the
    /// registry's `-2`, `-3` disambiguation) has to make room for its own
    /// suffix. Anything longer is truncated by [`KilnName::normalize`] and
    /// refused by [`KilnName::parse`].
    pub const MAX_LEN: usize = MAX_LEN;
}

/// A kiln name: the key of a `[kilns]` entry in the user's config.
///
/// Guaranteed by construction to be non-empty, at most [`MAX_LEN`] bytes, drawn
/// from `[A-Za-z0-9._- ]`, neither starting nor ending with a space, and not to
/// start with a dot — so it is never `.`, `..`, a hidden file, or anything
/// holding a path separator.
///
/// Two strings, not one, and the pair is the whole design: `display` is what
/// the user wrote and what every renderer shows; `key` is that text with ASCII
/// case folded, and it is the only field [`PartialEq`], [`Hash`] and [`Ord`]
/// read. So a map keyed by a name answers to any casing of it, and still hands
/// back the casing its owner chose.
#[derive(Debug, Clone)]
pub struct KilnName {
    display: String,
    key: String,
}

impl PartialEq for KilnName {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for KilnName {}

impl Hash for KilnName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl PartialOrd for KilnName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for KilnName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl Serialize for KilnName {
    /// The display spelling, so a name round-trips through a persisted
    /// `meta.json` or an RPC reply as the user registered it.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.display)
    }
}

/// Why a string is not usable as a kiln name.
///
/// Carries the rejected text: these become RPC errors and startup diagnostics,
/// and "invalid kiln name" with nothing else in it is unactionable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid kiln name {value:?}: {reason}")]
pub struct InvalidKilnName {
    /// The rejected string.
    pub value: String,
    /// Which rule it broke.
    pub reason: &'static str,
}

impl InvalidKilnName {
    fn new(value: &str, reason: &'static str) -> Self {
        Self {
            value: value.to_string(),
            reason,
        }
    }
}

impl KilnName {
    /// Fold text to the key that equality, hashing and ordering compare.
    ///
    /// ASCII case only. A name is a key a person types at a shell and reads in
    /// a picker, and Unicode case folding is locale-dependent in exactly the
    /// places (Turkish dotted `I`) where two users would disagree about which
    /// kiln they named.
    ///
    /// Public because the layers that key a *map* by the raw string — the
    /// registration overlay, `kilns.json`, the `[kilns]` table — have to agree
    /// with this type about when two names are one name.
    pub fn fold_str(raw: &str) -> String {
        raw.to_ascii_lowercase()
    }

    /// Validate `s` as a kiln name.
    ///
    /// An allowlist, not a denylist: anything outside `[A-Za-z0-9._- ]` is
    /// refused rather than enumerated as dangerous, so separators, encodings
    /// and the Windows spellings fall out for free instead of each needing a
    /// rule.
    pub fn parse(s: &str) -> Result<Self, InvalidKilnName> {
        if s.is_empty() {
            return Err(InvalidKilnName::new(s, "it is empty"));
        }
        if s.len() > MAX_LEN {
            return Err(InvalidKilnName::new(
                s,
                "it is longer than a kiln name may be",
            ));
        }
        if let Some(bad) = s
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ')))
        {
            // Named rather than echoed: a control character or NUL in the
            // message is worse than useless in a log.
            return Err(InvalidKilnName::new(
                s,
                match bad {
                    '/' | '\\' => "it contains a path separator",
                    '\0' => "it contains a NUL byte",
                    _ => "it contains a character outside [A-Za-z0-9._- ]",
                },
            ));
        }
        // A padded name renders as a name with a hole beside it, and two names
        // that differ only in padding are two entries the user reads as one.
        // `normalize` trims; `parse` refuses, so nothing is stored that the
        // user did not type.
        if s.starts_with(' ') || s.ends_with(' ') {
            return Err(InvalidKilnName::new(s, "it starts or ends with a space"));
        }
        // Excludes `.` and `..` as a side effect, and keeps a name out of the
        // hidden-file namespace so that a name can never be mistaken for one of
        // Crucible's own dot-directories.
        if s.starts_with('.') {
            return Err(InvalidKilnName::new(s, "it starts with a dot"));
        }
        Ok(Self {
            key: Self::fold_str(s),
            display: s.to_string(),
        })
    }

    /// Fold arbitrary text into a valid name, or `None` when nothing valid
    /// survives.
    ///
    /// This is the *registration* door: a hand-authored `[kilns]` key and a
    /// name derived from a directory basename both arrive here. Folding rather
    /// than refusing is a decision, not laziness — aborting the daemon over a
    /// key its own wizard wrote is a worse outcome than a name with a space in
    /// it.
    ///
    /// Case survives, and so does a space: `My Vault` stays `My Vault`, because
    /// the directory a user named is the label they expect to see. Only a
    /// character the charset has no room for becomes a space, one per run.
    ///
    /// `None` is the fail-closed answer and callers must treat it as "no name,
    /// therefore no kiln": `"/"`, `"…"` and `""` all fold to nothing, and a
    /// name is exactly what a caller must not be able to conjure out of one.
    pub fn normalize(raw: &str) -> Option<Self> {
        let mut folded = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                folded.push(ch);
            } else if !folded.ends_with(' ') {
                // One space per run of unrepresentable characters — and a run
                // of real spaces collapses the same way, so `My   Vault` is
                // `My Vault` rather than a name with a hole in it.
                folded.push(' ');
            }
        }
        // Leading dots and separators are trimmed rather than replaced: a
        // leading dot is refused by `parse`, and a leading `-` or space is
        // noise from whatever preceded the first real character.
        let trimmed = folded
            .trim_start_matches(['-', '.', ' '])
            .trim_end_matches(['-', ' ']);
        // Every character is ASCII by construction, so this cannot split a
        // char. Trim again: truncation can expose a trailing separator.
        let capped = trimmed
            .get(..MAX_LEN)
            .unwrap_or(trimmed)
            .trim_end_matches(['-', ' ']);
        Self::parse(capped).ok()
    }

    /// The name as the user registered it — the spelling every renderer shows.
    pub fn as_str(&self) -> &str {
        &self.display
    }

    /// The case-folded key two names are compared by.
    pub fn fold_key(&self) -> &str {
        &self.key
    }
}

impl fmt::Display for KilnName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display)
    }
}

impl FromStr for KilnName {
    type Err = InvalidKilnName;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Deref for KilnName {
    type Target = str;

    fn deref(&self) -> &str {
        &self.display
    }
}

impl AsRef<str> for KilnName {
    fn as_ref(&self) -> &str {
        &self.display
    }
}

impl PartialEq<str> for KilnName {
    /// Case-insensitive, like every other comparison of two names: a caller
    /// asking "is this the kiln called `docs`" means the kiln, not the casing.
    fn eq(&self, other: &str) -> bool {
        self.display.eq_ignore_ascii_case(other)
    }
}

impl PartialEq<&str> for KilnName {
    fn eq(&self, other: &&str) -> bool {
        self.display.eq_ignore_ascii_case(other)
    }
}

impl From<KilnName> for String {
    fn from(name: KilnName) -> String {
        name.display
    }
}

impl<'de> Deserialize<'de> for KilnName {
    /// Routes through [`KilnName::parse`], so a name arriving off the wire or
    /// out of a persisted file is validated by construction rather than by
    /// whichever handler remembered to check.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        KilnName::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_name_round_trips() {
        for ok in ["vault", "work-notes", "crucible-docs", "a", "n2", "a_b.c"] {
            assert_eq!(
                KilnName::parse(ok).map(|n| n.as_str().to_string()),
                Ok(ok.to_string()),
                "{ok:?} is an ordinary name"
            );
        }
        assert!(KilnName::parse(&"a".repeat(MAX_LEN)).is_ok());
    }

    /// The charset is the rule; each of these is refused by it rather than by a
    /// clause naming the specific trick.
    #[test]
    fn a_name_outside_the_charset_is_refused() {
        for hostile in [
            "",
            ".",
            "..",
            "../keys",
            "/etc",
            "a/b",
            "a\\b",
            "C:\\Windows",
            ".hidden",
            "caf\u{e9}",
            "name\0",
            "name\n",
            "na%2Fme",
        ] {
            assert!(
                KilnName::parse(hostile).is_err(),
                "{hostile:?} was accepted as a kiln name"
            );
        }
        assert!(KilnName::parse(&"a".repeat(MAX_LEN + 1)).is_err());
    }

    #[test]
    fn deserialization_refuses_what_parse_refuses() {
        assert!(serde_json::from_str::<KilnName>(r#""../keys""#).is_err());
        assert!(serde_json::from_str::<KilnName>(r#""/etc/passwd""#).is_err());
        assert!(serde_json::from_str::<KilnName>(r#""""#).is_err());
        assert!(serde_json::from_str::<KilnName>(r#"" Work""#).is_err());
        assert_eq!(
            serde_json::from_str::<KilnName>(r#""vault""#).unwrap(),
            KilnName::parse("vault").unwrap()
        );
        assert_eq!(
            serde_json::to_string(&KilnName::parse("vault").unwrap()).unwrap(),
            r#""vault""#
        );
    }

    /// A hand-written key folds; it does not abort. Case and spaces survive —
    /// only a character the charset has no room for is replaced.
    #[test]
    fn normalization_folds_a_hand_written_key_to_a_valid_name() {
        for (raw, expected) in [
            ("My Vault", "My Vault"),
            ("Work", "Work"),
            ("my   vault", "my vault"),
            ("Caf\u{e9} Notes", "Caf Notes"),
            ("-leading", "leading"),
            (".hidden", "hidden"),
            ("trailing-", "trailing"),
            ("a/b", "a b"),
        ] {
            assert_eq!(
                KilnName::normalize(raw).as_deref(),
                Some(expected),
                "{raw:?} folded wrong"
            );
        }
    }

    /// The fail-closed half: folding must never manufacture a name out of
    /// something that names no kiln. An empty key is the shape that once let an
    /// empty root permit every path.
    #[test]
    fn normalization_yields_nothing_when_nothing_valid_survives() {
        for nothing in [
            "", "/", "//", "...", "..", ".", "   ", "\u{2026}", "-", "-.-",
        ] {
            assert_eq!(
                KilnName::normalize(nothing),
                None,
                "{nothing:?} was folded into a name"
            );
        }
    }

    /// The property the registry leans on: whatever comes back from
    /// `normalize` is something `parse` accepts. Stated as a check over
    /// hostile inputs rather than trusted from the implementation, because the
    /// truncation step can re-introduce a trailing separator.
    #[test]
    fn a_folded_name_is_always_a_valid_name() {
        let long = "A".repeat(MAX_LEN * 3);
        let trailing_dash_at_the_cut = format!("{}-tail", "a".repeat(MAX_LEN - 1));
        for raw in [
            "My Vault",
            "../../etc",
            "/",
            "a/b/c",
            &long,
            &trailing_dash_at_the_cut,
            "\u{2026}\u{2026}",
            "Über Notes",
            "Crucible Help",
        ] {
            if let Some(name) = KilnName::normalize(raw) {
                assert_eq!(
                    KilnName::parse(name.as_str()).as_ref(),
                    Ok(&name),
                    "normalize({raw:?}) produced {name:?}, which parse refuses"
                );
            }
        }
    }

    #[test]
    fn an_over_long_key_is_truncated_rather_than_refused() {
        let name = KilnName::normalize(&"a".repeat(MAX_LEN * 2)).unwrap();
        assert_eq!(name.as_str().len(), MAX_LEN);
    }

    /// The user's own kiln is called "Crucible Help" — a capital C, a capital
    /// H and a space. A name is a label a person reads, and case-folding it at
    /// the door made every picker render a name nobody chose.
    #[test]
    fn a_name_keeps_its_case_and_may_hold_spaces() {
        for ok in ["Crucible Help", "Work", "My Vault", "a b c", "Notes.2024"] {
            assert_eq!(
                KilnName::parse(ok).map(|n| n.as_str().to_string()),
                Ok(ok.to_string()),
                "{ok:?} must round-trip exactly as written"
            );
        }
    }

    /// Case is kept for display and ignored for resolution: `Work` and `work`
    /// are one kiln, and the registered spelling is the one that renders.
    #[test]
    fn resolution_ignores_case_and_display_does_not() {
        let registered = KilnName::parse("Crucible Help").unwrap();
        let typed = KilnName::parse("crucible help").unwrap();

        assert_eq!(registered, typed, "one kiln, two spellings");
        assert_eq!(
            registered.as_str(),
            "Crucible Help",
            "the registered spelling is what renders"
        );

        let mut map = std::collections::BTreeMap::new();
        map.insert(registered.clone(), 1);
        assert_eq!(
            map.get(&typed),
            Some(&1),
            "a map must resolve either spelling"
        );

        let mut set = std::collections::HashSet::new();
        set.insert(registered.clone());
        assert!(set.contains(&typed), "hashing must agree with equality");
    }

    /// The widened charset is not a widened door: a name is still never a path,
    /// never hidden, and never padded.
    #[test]
    fn a_wider_charset_still_refuses_a_path_and_untrimmed_text() {
        for hostile in [
            "",
            " ",
            "   ",
            ".",
            "..",
            "../keys",
            "/etc",
            "a/b",
            "a\\b",
            "C:\\Windows",
            ".hidden",
            " leading",
            "trailing ",
            "caf\u{e9}",
            "name\0",
            "name\n",
            "na%2Fme",
        ] {
            assert!(
                KilnName::parse(hostile).is_err(),
                "{hostile:?} was accepted as a kiln name"
            );
        }
    }
}
