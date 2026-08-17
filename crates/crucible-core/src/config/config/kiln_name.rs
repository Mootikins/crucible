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
//! Modelled on [`SessionId`](crate::session::SessionId), with two deliberate
//! divergences from its charset:
//!
//! - **Lower case only** (`[a-z0-9._-]`, against `SessionId`'s
//!   `[A-Za-z0-9._-]`). A name is a *key the user types*, so `Work` and `work`
//!   must not be two kilns. Registration case-folds via [`KilnName::normalize`];
//!   [`KilnName::parse`] then refuses the un-folded spelling rather than
//!   quietly accepting a second one.
//! - **64 bytes, not 128.** A session id is a filesystem component and needs the
//!   headroom; a name is a map key and a prompt line.
//!
//! Deliberately **not** `AsRef<Path>`: a name is not a path, and the whole
//! point of the type is that no caller can turn one into a directory without
//! going through the registry.

use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Borrow;
use std::fmt;
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
/// from `[a-z0-9._-]`, and not to start with a dot — so it is never `.`, `..`,
/// a hidden file, or anything holding a path separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct KilnName(String);

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
    /// Validate `s` as a kiln name.
    ///
    /// An allowlist, not a denylist: anything outside `[a-z0-9._-]` is refused
    /// rather than enumerated as dangerous, so separators, encodings and the
    /// Windows spellings fall out for free instead of each needing a rule.
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
        if let Some(bad) = s.chars().find(|c| {
            !(c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
        }) {
            // Named rather than echoed: a control character or NUL in the
            // message is worse than useless in a log.
            return Err(InvalidKilnName::new(
                s,
                match bad {
                    '/' | '\\' => "it contains a path separator",
                    '\0' => "it contains a NUL byte",
                    c if c.is_ascii_uppercase() => {
                        "it contains an upper-case letter; kiln names are case-folded"
                    }
                    _ => "it contains a character outside [a-z0-9._-]",
                },
            ));
        }
        // Excludes `.` and `..` as a side effect, and keeps a name out of the
        // hidden-file namespace so that a name can never be mistaken for one of
        // Crucible's own dot-directories.
        if s.starts_with('.') {
            return Err(InvalidKilnName::new(s, "it starts with a dot"));
        }
        Ok(Self(s.to_string()))
    }

    /// Fold arbitrary text into a valid name, or `None` when nothing valid
    /// survives.
    ///
    /// This is the *registration* door: a hand-authored `[kilns]` key written
    /// before names were validated (`cru init` writes the directory basename
    /// verbatim, so `My Vault` exists in the wild) and a name derived from a
    /// directory basename both arrive here. Folding rather than refusing is a
    /// decision, not laziness — aborting the daemon over a key its own wizard
    /// wrote is a worse outcome than `my-vault`.
    ///
    /// `None` is the fail-closed answer and callers must treat it as "no name,
    /// therefore no kiln": `"/"`, `"…"` and `""` all fold to nothing, and a
    /// name is exactly what a caller must not be able to conjure out of one.
    pub fn normalize(raw: &str) -> Option<Self> {
        let mut folded = String::with_capacity(raw.len());
        for ch in raw.chars() {
            let lowered = ch.to_ascii_lowercase();
            if lowered.is_ascii_lowercase()
                || lowered.is_ascii_digit()
                || matches!(lowered, '.' | '_' | '-')
            {
                folded.push(lowered);
            } else if !folded.ends_with('-') {
                // One separator per run of unrepresentable characters, so
                // `My   Vault` is `my-vault` rather than `my---vault`.
                folded.push('-');
            }
        }
        // Leading dots and separators are trimmed rather than replaced: a
        // leading dot is refused by `parse`, and a leading `-` is noise from
        // whatever preceded the first real character.
        let trimmed = folded.trim_start_matches(['-', '.']).trim_end_matches('-');
        // Every character is ASCII by construction, so this cannot split a
        // char. Trim again: truncation can expose a trailing separator.
        let capped = trimmed
            .get(..MAX_LEN)
            .unwrap_or(trimmed)
            .trim_end_matches('-');
        Self::parse(capped).ok()
    }

    /// The name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KilnName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
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
        &self.0
    }
}

impl AsRef<str> for KilnName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for KilnName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for KilnName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for KilnName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl From<KilnName> for String {
    fn from(name: KilnName) -> String {
        name.0
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
            "my vault",
            "My Vault",
            "Work",
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
        assert!(serde_json::from_str::<KilnName>(r#""Work""#).is_err());
        assert_eq!(
            serde_json::from_str::<KilnName>(r#""vault""#).unwrap(),
            KilnName::parse("vault").unwrap()
        );
        assert_eq!(
            serde_json::to_string(&KilnName::parse("vault").unwrap()).unwrap(),
            r#""vault""#
        );
    }

    /// `cru init` writes the directory basename verbatim, so keys like
    /// `My Vault` are already in users' configs. They fold; they do not abort.
    #[test]
    fn normalization_folds_a_hand_written_key_to_a_valid_name() {
        for (raw, expected) in [
            ("My Vault", "my-vault"),
            ("Work", "work"),
            ("my   vault", "my-vault"),
            ("Caf\u{e9} Notes", "caf-notes"),
            ("-leading", "leading"),
            (".hidden", "hidden"),
            ("trailing-", "trailing"),
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
}
