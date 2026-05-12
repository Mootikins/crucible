//! Memory scoping for notes (security boundary).
//!
//! Every note carries a `Scope` in its frontmatter / `properties.scope` field.
//! Every read against the [`NoteStore`](crate::storage::NoteStore) carries an
//! **authority** scope (typically derived from the active session). A note is
//! visible to a request iff [`Scope::can_read`] returns `true`.
//!
//! # Visibility matrix
//!
//! | Authority \ Note scope | `Global` | `Workspace{p}` | `User{id}` |
//! |---|---|---|---|
//! | `Global`               | yes      | yes (any p)    | yes (any id) |
//! | `Workspace{a}`         | yes      | yes iff p == a | no |
//! | `User{alice}`          | yes      | no             | yes iff id == alice |
//!
//! Rationale:
//! - `Global` is the "elevated" authority — admin / cross-cutting agents.
//! - Sibling workspaces are default-deny: workspace `a` cannot see workspace `b`.
//! - User-scoped notes are personal: even within a workspace, user-scoped data
//!   only flows back to the same user. This is the strictest scope.
//!
//! # Write side
//!
//! On write, the proposed note scope must be **no broader** than the writing
//! session's authority. See [`Scope::can_write`].
//!
//! # Default for legacy notes
//!
//! Notes without an explicit `scope:` frontmatter property get a `Workspace`
//! scope derived from the kiln they live in at upsert time. The default is
//! stamped onto the in-memory [`NoteRecord`] — markdown files on disk are
//! never mutated.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Memory scope for a note or a request authority.
///
/// Stored on a [`NoteRecord`](crate::storage::NoteRecord) as a string in
/// `properties.scope`. Encoded/decoded via [`Scope::to_property_value`] /
/// [`Scope::from_property_value`].
///
/// # Wire format (JSON)
///
/// ```text
/// {"kind":"global"}
/// {"kind":"workspace","path":"/abs/path"}
/// {"kind":"user","id":"alice"}
/// ```
///
/// # Frontmatter format
///
/// ```text
/// scope: global
/// scope: workspace          # path inferred from kiln binding
/// scope: workspace:/foo     # explicit workspace path
/// scope: user:alice         # explicit user id
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    /// The note is globally visible across all workspaces and users. Lowest
    /// confidentiality, highest reach. Anything a session can read.
    Global,
    /// The note is private to a single workspace. Sibling workspaces cannot
    /// see it. Path is canonicalized at construction; equality is
    /// path-string equality.
    Workspace {
        /// Absolute, canonical workspace path. Use [`Scope::workspace`] to
        /// construct — it canonicalizes for you.
        path: PathBuf,
    },
    /// The note is private to a single named user. Most restrictive scope.
    User {
        /// Stable identifier for the user (email, GitHub login, etc.).
        id: String,
    },
}

impl Scope {
    /// Construct a workspace scope, canonicalizing the path when possible.
    ///
    /// If the path doesn't exist or canonicalization fails, the path is
    /// stored as-is. Equality between two `Workspace` scopes is exact
    /// path-string equality after canonicalization, so callers should
    /// always construct via this helper.
    pub fn workspace(path: impl AsRef<Path>) -> Self {
        let p = path.as_ref();
        let canon = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        Scope::Workspace { path: canon }
    }

    /// Construct a user scope.
    pub fn user(id: impl Into<String>) -> Self {
        Scope::User { id: id.into() }
    }

    /// Construct the elevated (global) scope.
    pub fn global() -> Self {
        Scope::Global
    }

    /// Can a request with `self` as authority **read** a note with `note_scope`?
    ///
    /// This is the central security predicate. See module docs for the
    /// visibility matrix. The rule is conservative — any deviation should be
    /// reviewed as a security regression.
    pub fn can_read(&self, note_scope: &Scope) -> bool {
        match (self, note_scope) {
            // Global authority sees everything.
            (Scope::Global, _) => true,
            // Anyone can see global notes.
            (_, Scope::Global) => true,
            // Workspace ↔ Workspace: only same-path.
            (Scope::Workspace { path: a }, Scope::Workspace { path: b }) => a == b,
            // User ↔ User: only same-id.
            (Scope::User { id: a }, Scope::User { id: b }) => a == b,
            // Workspace authority cannot see user-scoped notes — user is more
            // restrictive than workspace.
            (Scope::Workspace { .. }, Scope::User { .. }) => false,
            // User authority cannot see other workspaces' notes — workspace
            // data is shared across that workspace's users; we don't leak it
            // to a user with no workspace claim.
            (Scope::User { .. }, Scope::Workspace { .. }) => false,
        }
    }

    /// Can a session with `self` as authority **write** a note with `note_scope`?
    ///
    /// Write is allowed when the proposed scope is no broader than the
    /// session's authority. Concretely:
    /// - `Global` authority can write any scope.
    /// - `Workspace{a}` can write `Workspace{a}` or `User{*}` (narrower).
    /// - `User{alice}` can write `User{alice}` only — even within a workspace.
    ///
    /// Returns `false` for any attempt to **broaden** scope on write (e.g.
    /// a workspace session writing `Global`, or a user writing `Workspace`).
    pub fn can_write(&self, note_scope: &Scope) -> bool {
        match (self, note_scope) {
            (Scope::Global, _) => true,
            (Scope::Workspace { path: a }, Scope::Workspace { path: b }) => a == b,
            (Scope::Workspace { .. }, Scope::User { .. }) => true,
            (Scope::Workspace { .. }, Scope::Global) => false,
            (Scope::User { id: a }, Scope::User { id: b }) => a == b,
            (Scope::User { .. }, _) => false,
        }
    }

    /// Encode this scope as a JSON value suitable for storing in
    /// `NoteRecord.properties["scope"]`. The shape is the same as the
    /// `serde(tag = "kind")` representation on the enum itself.
    pub fn to_property_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Decode a scope from a `NoteRecord.properties["scope"]` value.
    ///
    /// Accepts the structured object form (`{"kind":"workspace","path":...}`)
    /// and the relaxed frontmatter string form (`"global"`, `"workspace"`,
    /// `"workspace:/path"`, `"user:alice"`).
    ///
    /// Returns `None` if the value is missing, null, or unparseable. Callers
    /// should fall back to the derived default in that case.
    pub fn from_property_value(value: &serde_json::Value) -> Option<Scope> {
        // Structured form.
        if value.is_object() {
            if let Ok(scope) = serde_json::from_value::<Scope>(value.clone()) {
                return Some(scope);
            }
        }
        // Relaxed string form (what a user types into frontmatter).
        if let Some(s) = value.as_str() {
            return Self::from_frontmatter_str(s);
        }
        None
    }

    /// Parse a frontmatter `scope:` string.
    ///
    /// Accepted forms (case-insensitive on the kind tag):
    /// - `"global"` → `Scope::Global`
    /// - `"workspace"` → `Scope::Workspace { path: "" }` (path filled in by
    ///   the pipeline from the kiln binding at upsert time)
    /// - `"workspace:/abs/path"` → `Scope::Workspace { path: "/abs/path" }`
    /// - `"user:alice"` → `Scope::User { id: "alice" }`
    pub fn from_frontmatter_str(s: &str) -> Option<Scope> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return None;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower == "global" {
            return Some(Scope::Global);
        }
        if lower == "workspace" {
            // Path filled in later by the pipeline.
            return Some(Scope::Workspace {
                path: PathBuf::new(),
            });
        }
        if let Some(rest) = trimmed.strip_prefix("workspace:") {
            return Some(Scope::Workspace {
                path: PathBuf::from(rest.trim()),
            });
        }
        if let Some(rest) = trimmed.strip_prefix("user:") {
            let id = rest.trim();
            if id.is_empty() {
                return None;
            }
            return Some(Scope::User { id: id.to_string() });
        }
        None
    }

    /// Returns true if this is a `Workspace` scope with an empty path —
    /// i.e. a frontmatter-derived placeholder that needs filling in.
    pub fn is_unbound_workspace(&self) -> bool {
        matches!(self, Scope::Workspace { path } if path.as_os_str().is_empty())
    }

    /// Replace an empty `Workspace { path: "" }` placeholder with the given
    /// kiln path. If `self` is anything else, returns `self` unchanged.
    ///
    /// Used by the note pipeline to bind frontmatter `scope: workspace`
    /// declarations to the kiln they were written in.
    #[must_use]
    pub fn bind_to_workspace(self, kiln_path: &Path) -> Self {
        if self.is_unbound_workspace() {
            return Scope::workspace(kiln_path);
        }
        self
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::Global => write!(f, "global"),
            Scope::Workspace { path } => write!(f, "workspace:{}", path.display()),
            Scope::User { id } => write!(f, "user:{}", id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(p: &str) -> Scope {
        // Raw constructor — do not canonicalize so tests stay deterministic.
        Scope::Workspace {
            path: PathBuf::from(p),
        }
    }

    #[test]
    fn global_authority_sees_everything() {
        assert!(Scope::Global.can_read(&Scope::Global));
        assert!(Scope::Global.can_read(&ws("/a")));
        assert!(Scope::Global.can_read(&Scope::user("alice")));
    }

    #[test]
    fn anyone_sees_global_notes() {
        assert!(ws("/a").can_read(&Scope::Global));
        assert!(Scope::user("alice").can_read(&Scope::Global));
    }

    #[test]
    fn sibling_workspaces_are_isolated() {
        assert!(!ws("/a").can_read(&ws("/b")));
        assert!(!ws("/b").can_read(&ws("/a")));
        assert!(ws("/a").can_read(&ws("/a")));
    }

    #[test]
    fn user_scopes_are_isolated() {
        assert!(!Scope::user("alice").can_read(&Scope::user("bob")));
        assert!(Scope::user("alice").can_read(&Scope::user("alice")));
    }

    #[test]
    fn workspace_cannot_see_user_notes() {
        // Even within the "same workspace context", user-scoped notes are
        // private. Workspace can't peek into user data.
        assert!(!ws("/a").can_read(&Scope::user("alice")));
    }

    #[test]
    fn user_cannot_see_workspace_notes() {
        // The reverse: a user-only session does not have workspace authority.
        assert!(!Scope::user("alice").can_read(&ws("/a")));
    }

    #[test]
    fn can_write_workspace_to_user_narrower() {
        assert!(ws("/a").can_write(&Scope::user("alice")));
    }

    #[test]
    fn can_write_workspace_to_global_rejected() {
        // Adversarial: a workspace session must not write a global note.
        assert!(!ws("/a").can_write(&Scope::Global));
    }

    #[test]
    fn can_write_workspace_to_sibling_rejected() {
        assert!(!ws("/a").can_write(&ws("/b")));
    }

    #[test]
    fn can_write_user_only_self() {
        assert!(Scope::user("alice").can_write(&Scope::user("alice")));
        assert!(!Scope::user("alice").can_write(&Scope::user("bob")));
        assert!(!Scope::user("alice").can_write(&Scope::Global));
        assert!(!Scope::user("alice").can_write(&ws("/a")));
    }

    #[test]
    fn global_can_write_anything() {
        assert!(Scope::Global.can_write(&Scope::Global));
        assert!(Scope::Global.can_write(&ws("/a")));
        assert!(Scope::Global.can_write(&Scope::user("alice")));
    }

    #[test]
    fn frontmatter_parse_global() {
        assert_eq!(Scope::from_frontmatter_str("global"), Some(Scope::Global));
        assert_eq!(Scope::from_frontmatter_str("Global"), Some(Scope::Global));
    }

    #[test]
    fn frontmatter_parse_workspace_unbound() {
        assert_eq!(
            Scope::from_frontmatter_str("workspace"),
            Some(Scope::Workspace {
                path: PathBuf::new()
            })
        );
    }

    #[test]
    fn frontmatter_parse_workspace_with_path() {
        assert_eq!(
            Scope::from_frontmatter_str("workspace:/foo/bar"),
            Some(ws("/foo/bar"))
        );
    }

    #[test]
    fn frontmatter_parse_user() {
        assert_eq!(
            Scope::from_frontmatter_str("user:alice"),
            Some(Scope::user("alice"))
        );
    }

    #[test]
    fn frontmatter_parse_user_empty_rejected() {
        assert_eq!(Scope::from_frontmatter_str("user:"), None);
    }

    #[test]
    fn frontmatter_parse_unknown_rejected() {
        assert_eq!(Scope::from_frontmatter_str("admin"), None);
        assert_eq!(Scope::from_frontmatter_str(""), None);
    }

    #[test]
    fn bind_to_workspace_fills_empty_placeholder() {
        let unbound = Scope::Workspace {
            path: PathBuf::new(),
        };
        let bound = unbound.bind_to_workspace(Path::new("/k"));
        match bound {
            Scope::Workspace { path } => assert_eq!(path, PathBuf::from("/k")),
            _ => panic!("expected workspace"),
        }
    }

    #[test]
    fn bind_to_workspace_leaves_other_scopes_alone() {
        assert_eq!(
            Scope::Global.bind_to_workspace(Path::new("/k")),
            Scope::Global
        );
        assert_eq!(
            Scope::user("alice").bind_to_workspace(Path::new("/k")),
            Scope::user("alice")
        );
        assert_eq!(ws("/a").bind_to_workspace(Path::new("/k")), ws("/a"));
    }

    #[test]
    fn property_value_roundtrip_global() {
        let s = Scope::Global;
        let v = s.to_property_value();
        let decoded = Scope::from_property_value(&v).expect("decode");
        assert_eq!(decoded, s);
    }

    #[test]
    fn property_value_roundtrip_workspace() {
        let s = ws("/x/y");
        let v = s.to_property_value();
        let decoded = Scope::from_property_value(&v).expect("decode");
        assert_eq!(decoded, s);
    }

    #[test]
    fn property_value_roundtrip_user() {
        let s = Scope::user("alice");
        let v = s.to_property_value();
        let decoded = Scope::from_property_value(&v).expect("decode");
        assert_eq!(decoded, s);
    }

    #[test]
    fn property_value_decodes_relaxed_string() {
        let v = serde_json::Value::String("user:alice".into());
        let decoded = Scope::from_property_value(&v).expect("decode");
        assert_eq!(decoded, Scope::user("alice"));
    }

    #[test]
    fn property_value_decodes_missing_as_none() {
        assert_eq!(Scope::from_property_value(&serde_json::Value::Null), None);
    }

    #[test]
    fn display_global() {
        assert_eq!(Scope::Global.to_string(), "global");
    }

    #[test]
    fn display_workspace() {
        assert_eq!(ws("/foo").to_string(), "workspace:/foo");
    }

    #[test]
    fn display_user() {
        assert_eq!(Scope::user("alice").to_string(), "user:alice");
    }
}
