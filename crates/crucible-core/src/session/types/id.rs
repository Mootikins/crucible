//! The validated session identifier.
//!
//! A session id is not free-form text: every one of them is joined onto a
//! daemon-owned root to name a directory (`{sessions_root}/{id}`), and the
//! sinks on the other side of that join include `remove_dir_all`. `Path::join`
//! normalizes nothing and an *absolute* component replaces the base outright,
//! so `"../../Documents"` and `"/etc"` are both spellings of "any directory
//! this process can reach".
//!
//! [`SessionId`] is therefore the only type allowed to name a session
//! directory, and the only way to build one is [`SessionId::parse`]. Ids arrive
//! from two doors and both go through it:
//!
//! - **RPC parameters** — a `session_id` string off the wire.
//! - **`meta.json`** — the `id` *field inside* a persisted session, which is
//!   not the same string as the directory it was found in. A kiln shared or
//!   synced between machines carries that file, so its contents are caller
//!   input; migration used to publish it verbatim.
//!
//! `Deserialize` routes through `parse`, so the second door is closed by
//! construction: a poisoned `meta.json` fails to load rather than loading into
//! a `String` that later becomes a path.

use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use super::enums::SessionType;

/// The longest id we will accept. Well above anything we mint (~26 bytes) and
/// below every filesystem's per-component limit (255 on ext4/APFS/NTFS), so a
/// valid id never fails at `create_dir` for length.
const MAX_LEN: usize = 128;

/// A session identifier that is safe to join onto a directory.
///
/// Guaranteed by construction to be a single, ordinary path component:
/// non-empty, no separators, no `.`/`..`, no NUL, no leading dot, and drawn
/// from `[A-Za-z0-9._-]`. See the module docs for why that matters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SessionId(String);

/// Why a string is not usable as a session id.
///
/// Carries the rejected text: these turn into RPC errors and log lines, and
/// "invalid session id" with nothing else in it is unactionable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid session id {value:?}: {reason}")]
pub struct InvalidSessionId {
    /// The rejected string.
    pub value: String,
    /// Which rule it broke.
    pub reason: &'static str,
}

impl InvalidSessionId {
    fn new(value: &str, reason: &'static str) -> Self {
        Self {
            value: value.to_string(),
            reason,
        }
    }
}

impl SessionId {
    /// Validate `s` as a session id.
    ///
    /// An allowlist, not a denylist: anything outside `[A-Za-z0-9._-]` is
    /// refused rather than enumerated as dangerous. That is what makes the
    /// Windows spellings (`C:evil`, `a\b`) and the encoding tricks fall out for
    /// free instead of needing their own rule.
    pub fn parse(s: &str) -> Result<Self, InvalidSessionId> {
        if s.is_empty() {
            return Err(InvalidSessionId::new(s, "it is empty"));
        }
        if s.len() > MAX_LEN {
            return Err(InvalidSessionId::new(
                s,
                "it is longer than a filesystem path component may be",
            ));
        }
        if let Some(bad) = s
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        {
            // Named rather than echoed: a control character or NUL in the
            // message is worse than useless in a log.
            return Err(InvalidSessionId::new(
                s,
                match bad {
                    '/' | '\\' => "it contains a path separator",
                    '\0' => "it contains a NUL byte",
                    _ => "it contains a character outside [A-Za-z0-9._-]",
                },
            ));
        }
        // `.` and `..` are already excluded by the leading-dot rule, which also
        // keeps ids out of the hidden-file namespace the sessions root uses for
        // its own bookkeeping (`.migrating`).
        if s.starts_with('.') {
            return Err(InvalidSessionId::new(s, "it starts with a dot"));
        }

        // The allowlist above already implies this. It is checked anyway
        // because it is the property the *callers* depend on, and a future
        // widening of the allowlist must not be able to break it silently.
        let mut components = Path::new(s).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => Ok(Self(s.to_string())),
            _ => Err(InvalidSessionId::new(
                s,
                "it is not a single ordinary path component",
            )),
        }
    }

    /// Mint a fresh id for a session of this type.
    ///
    /// The one constructor that does not validate, because it does not have to:
    /// the format is `{prefix}-{timestamp}-{6 alphanumerics}` and every part of
    /// it is inside the allowlist. A unit test pins that, so a change to the
    /// format that broke it fails loudly rather than at the next `create_dir`.
    pub fn generate(session_type: SessionType) -> Self {
        Self(super::agent::generate_session_id(session_type.as_prefix()))
    }

    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The session type this id was minted for, when its prefix names one.
    ///
    /// `Option` rather than a guarantee: an id is only ever *required* to be a
    /// path component. `replay-<uuid>`, a hand-filed directory, and a
    /// migration-suffixed id from a build that spelled types differently are
    /// all valid ids with nothing this can read. The old daemon-side parser
    /// returned this infallibly and paid for it with an `expect`.
    pub fn session_type(&self) -> Option<SessionType> {
        self.0
            .split('-')
            .next()
            .and_then(|prefix| prefix.parse().ok())
    }

    /// This session's directory under `root`.
    ///
    /// The only sanctioned way to turn an id into a path. It is a method on the
    /// validated type rather than a free function taking `&str` so that
    /// "where do session directories come from" has exactly one answer a
    /// reviewer can grep for.
    pub fn dir_under(&self, root: &Path) -> PathBuf {
        root.join(&self.0)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SessionId {
    type Err = InvalidSessionId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Deref for SessionId {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for SessionId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for SessionId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for SessionId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for SessionId {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl PartialEq<SessionId> for String {
    fn eq(&self, other: &SessionId) -> bool {
        self == &other.0
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> String {
        id.0
    }
}

impl From<&SessionId> for String {
    fn from(id: &SessionId) -> String {
        id.0.clone()
    }
}

impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        SessionId::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_id_is_a_valid_id() {
        for session_type in [SessionType::Chat, SessionType::Agent, SessionType::Workflow] {
            let id = SessionId::generate(session_type);
            assert_eq!(
                SessionId::parse(id.as_str()).as_ref(),
                Ok(&id),
                "the generator produced an id its own validator rejects"
            );
        }
    }

    #[test]
    fn an_id_reports_the_type_its_prefix_names_and_nothing_more() {
        for session_type in [SessionType::Chat, SessionType::Agent, SessionType::Workflow] {
            assert_eq!(
                SessionId::generate(session_type).session_type(),
                Some(session_type)
            );
        }
        assert_eq!(SessionId::parse("replay-abc").unwrap().session_type(), None);
    }

    #[test]
    fn an_ordinary_id_round_trips() {
        let id = SessionId::parse("chat-2026-08-15T1430-a1b2c3").unwrap();
        assert_eq!(id.as_str(), "chat-2026-08-15T1430-a1b2c3");
        // The migration suffix shape has to survive too.
        assert!(SessionId::parse("chat-1--deadbeef").is_ok());
    }

    #[test]
    fn a_traversing_id_is_refused() {
        for hostile in [
            "..",
            ".",
            "../keys",
            "../../Documents",
            "a/../../b",
            "foo/bar",
            "/etc",
            "/",
            "C:\\Windows",
            "a\\b",
            "..\\..\\keys",
        ] {
            assert!(
                SessionId::parse(hostile).is_err(),
                "{hostile:?} was accepted as a session id"
            );
        }
    }

    #[test]
    fn an_empty_or_hidden_or_oversized_id_is_refused() {
        assert!(SessionId::parse("").is_err());
        assert!(SessionId::parse(".migrating").is_err());
        assert!(SessionId::parse(".hidden").is_err());
        assert!(SessionId::parse(&"a".repeat(MAX_LEN + 1)).is_err());
        assert!(SessionId::parse(&"a".repeat(MAX_LEN)).is_ok());
    }

    #[test]
    fn an_id_carrying_a_nul_or_control_byte_is_refused() {
        assert!(SessionId::parse("chat-1\0/etc").is_err());
        assert!(SessionId::parse("chat\n1").is_err());
        assert!(SessionId::parse("chat 1").is_err());
        // Percent- and unicode-encoded separators are not separators to the
        // allowlist, they are simply not in it.
        assert!(SessionId::parse("chat%2F..").is_err());
        assert!(SessionId::parse("chat\u{2044}1").is_err());
    }

    #[test]
    fn deserialization_refuses_what_parse_refuses() {
        assert!(serde_json::from_str::<SessionId>(r#""../keys""#).is_err());
        assert!(serde_json::from_str::<SessionId>(r#""/etc/passwd""#).is_err());
        assert!(serde_json::from_str::<SessionId>(r#""""#).is_err());
        assert_eq!(
            serde_json::from_str::<SessionId>(r#""chat-1""#).unwrap(),
            SessionId::parse("chat-1").unwrap()
        );
    }

    #[test]
    fn a_directory_built_from_an_id_stays_directly_under_its_root() {
        let root = Path::new("/data/sessions");
        let dir = SessionId::parse("chat-1").unwrap().dir_under(root);
        assert_eq!(dir, Path::new("/data/sessions/chat-1"));
        assert_eq!(dir.parent(), Some(root));
    }
}
