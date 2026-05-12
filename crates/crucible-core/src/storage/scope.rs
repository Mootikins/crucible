//! Memory scoping for notes (security boundary).
//!
//! Every note carries a `Scope` in its frontmatter / `properties.scope` field,
//! tied to the workspace it was written in. Every read against the
//! [`NoteStore`](crate::storage::NoteStore) carries an **authority** scope
//! derived from the active session. A note is visible to a request iff the
//! two scopes match on canonical workspace path.
//!
//! # Visibility rule
//!
//! Sibling workspaces are default-deny: workspace `a` cannot see workspace `b`.
//! A note is visible iff `authority.same_workspace(note_scope)` returns true.
//!
//! # Default for legacy notes
//!
//! Notes without an explicit `scope:` frontmatter property get a workspace
//! scope derived from the kiln they live in at upsert time. The default is
//! stamped onto the in-memory [`NoteRecord`] — markdown files on disk are
//! never mutated.
//!
//! # Wave 2 prune
//!
//! Pre-prune this enum had `Global` and `User { id }` variants, intended to
//! support cross-cutting admin reads and per-user notes. Neither had any
//! in-tree consumer outside the (now removed) session-digest plugin; both
//! were collapsed into the single workspace-only variant. See the
//! consolidation pass commit for the rationale.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur constructing a [`Scope`].
#[derive(Debug, Error)]
pub enum ScopeError {
    /// The supplied path could not be canonicalized (typically: it doesn't
    /// exist, or a symlink along the path is broken). We refuse silent
    /// fallback to the unresolved path — see [`Scope::workspace`] for why.
    #[error("scope path cannot be canonicalized: {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A frontmatter `scope:` string used a kind that's no longer supported
    /// (e.g. `global`, `user:alice`). The Wave 2 prune dropped both.
    #[error("unsupported scope: {0}")]
    Unsupported(String),
}

/// Memory scope for a note or a request authority.
///
/// Stored on a [`NoteRecord`](crate::storage::NoteRecord) as a string in
/// `properties.scope`. Encoded/decoded via [`Scope::to_property_value`] /
/// [`Scope::from_property_value`].
///
/// # Wire format (JSON)
///
/// ```text
/// {"kind":"workspace","path":"/abs/path"}
/// ```
///
/// # Frontmatter format
///
/// ```text
/// scope: workspace          # path inferred from kiln binding
/// scope: workspace:/foo     # explicit workspace path
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    /// The note is private to a single workspace. Sibling workspaces cannot
    /// see it. Path is canonicalized at construction; equality is
    /// path-string equality.
    Workspace {
        /// Absolute, canonical workspace path. Use [`Scope::workspace`] or
        /// [`Scope::workspace_unchecked`] to construct.
        path: PathBuf,
    },
}

impl Scope {
    /// Construct a workspace scope, canonicalizing the path.
    ///
    /// Returns `Err(ScopeError::Canonicalize)` if the path cannot be
    /// canonicalized — typically because it doesn't exist. Failing loudly
    /// here avoids the asymmetric-path adversarial case where two scopes
    /// pointing at the same workspace via different unresolved spellings
    /// compare unequal.
    pub fn workspace(path: impl AsRef<Path>) -> Result<Self, ScopeError> {
        let p = path.as_ref();
        let canon = p.canonicalize().map_err(|source| ScopeError::Canonicalize {
            path: p.to_path_buf(),
            source,
        })?;
        Ok(Scope::Workspace { path: canon })
    }

    /// Construct a workspace scope without canonicalizing.
    ///
    /// Use only for test / placeholder paths that intentionally don't exist
    /// on disk (e.g. unbound frontmatter `scope: workspace` waiting to be
    /// bound by the pipeline). Production read/write paths should prefer
    /// [`Self::workspace`] so canonicalization failures surface immediately.
    pub fn workspace_unchecked(path: impl Into<PathBuf>) -> Self {
        Scope::Workspace { path: path.into() }
    }

    /// Read the workspace path. Always succeeds (there's exactly one variant).
    pub fn path(&self) -> &Path {
        let Scope::Workspace { path } = self;
        path
    }

    /// True iff `self` and `other` reference the same canonical workspace path.
    ///
    /// This is the central security predicate. A note is visible iff its
    /// stamped scope `same_workspace` the request's authority.
    pub fn same_workspace(&self, other: &Scope) -> bool {
        self.path() == other.path()
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
    /// and the relaxed frontmatter string form (`"workspace"`,
    /// `"workspace:/path"`).
    ///
    /// Returns `None` if the value is missing, null, or unparseable. Returns
    /// `Some(Err(...))` if the value matches a no-longer-supported scope
    /// (e.g. `"global"`, `"user:alice"`) — callers decide whether to surface
    /// the error or quarantine the note.
    ///
    /// **Strict refusal**: the Wave 2 prune removed `Global` and `User`
    /// variants; frontmatter using those kinds is now an error rather than
    /// being silently coerced.
    pub fn from_property_value(value: &serde_json::Value) -> Option<Result<Scope, ScopeError>> {
        // Structured form.
        if value.is_object() {
            // Accept current (workspace-only) shape.
            if let Ok(scope) = serde_json::from_value::<Scope>(value.clone()) {
                return Some(Ok(scope));
            }
            // Detect removed kinds and refuse explicitly.
            if let Some(kind) = value.get("kind").and_then(|k| k.as_str()) {
                if matches!(kind, "global" | "user") {
                    return Some(Err(ScopeError::Unsupported(kind.to_string())));
                }
            }
        }
        // Relaxed string form (what a user types into frontmatter).
        if let Some(s) = value.as_str() {
            return Some(Self::from_frontmatter_str(s));
        }
        None
    }

    /// Parse a frontmatter `scope:` string.
    ///
    /// Accepted forms (case-insensitive on the kind tag):
    /// - `"workspace"` → `Scope::Workspace { path: "" }` (path filled in by
    ///   the pipeline from the kiln binding at upsert time)
    /// - `"workspace:/abs/path"` → `Scope::Workspace { path: "/abs/path" }`
    ///
    /// Returns `Err(ScopeError::Unsupported)` for the legacy `global` and
    /// `user:*` kinds — the Wave 2 prune dropped them and we refuse silent
    /// coercion.
    pub fn from_frontmatter_str(s: &str) -> Result<Scope, ScopeError> {
        let trimmed = s.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower == "workspace" {
            // Path filled in later by the pipeline.
            return Ok(Scope::Workspace {
                path: PathBuf::new(),
            });
        }
        if let Some(rest) = trimmed.strip_prefix("workspace:") {
            return Ok(Scope::Workspace {
                path: PathBuf::from(rest.trim()),
            });
        }
        // Reject the removed kinds explicitly so a stale note doesn't
        // silently parse with the wrong semantics.
        if lower == "global" || lower.starts_with("user:") {
            return Err(ScopeError::Unsupported(trimmed.to_string()));
        }
        Err(ScopeError::Unsupported(trimmed.to_string()))
    }

    /// Returns true if this is a `Workspace` scope with an empty path —
    /// i.e. a frontmatter-derived placeholder that needs filling in.
    pub fn is_unbound_workspace(&self) -> bool {
        matches!(self, Scope::Workspace { path } if path.as_os_str().is_empty())
    }

    /// Replace an empty `Workspace { path: "" }` placeholder with the given
    /// kiln path. If `self` is already bound, returns `self` unchanged.
    ///
    /// Used by the note pipeline to bind frontmatter `scope: workspace`
    /// declarations to the kiln they were written in. The kiln path is
    /// canonicalized; if canonicalization fails the original path is kept
    /// (the pipeline never panics on a malformed kiln binding).
    #[must_use]
    pub fn bind_to_workspace(self, kiln_path: &Path) -> Self {
        if self.is_unbound_workspace() {
            let canon = kiln_path
                .canonicalize()
                .unwrap_or_else(|_| kiln_path.to_path_buf());
            return Scope::Workspace { path: canon };
        }
        self
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Scope::Workspace { path } = self;
        write!(f, "workspace:{}", path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(p: &str) -> Scope {
        // Raw constructor — do not canonicalize so tests stay deterministic.
        Scope::workspace_unchecked(p)
    }

    #[test]
    fn same_workspace_is_path_equality() {
        assert!(ws("/a").same_workspace(&ws("/a")));
        assert!(!ws("/a").same_workspace(&ws("/b")));
    }

    #[test]
    fn workspace_canonicalize_fails_on_missing_path() {
        let result = Scope::workspace("/definitely/does/not/exist/xyz");
        assert!(matches!(result, Err(ScopeError::Canonicalize { .. })));
    }

    #[test]
    fn workspace_canonicalize_succeeds_on_existing_path() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = Scope::workspace(tmp.path()).unwrap();
        assert_eq!(scope.path(), tmp.path().canonicalize().unwrap());
    }

    #[test]
    fn frontmatter_parse_workspace_unbound() {
        assert_eq!(
            Scope::from_frontmatter_str("workspace").unwrap(),
            Scope::Workspace {
                path: PathBuf::new()
            }
        );
    }

    #[test]
    fn frontmatter_parse_workspace_with_path() {
        assert_eq!(
            Scope::from_frontmatter_str("workspace:/foo/bar").unwrap(),
            ws("/foo/bar")
        );
    }

    #[test]
    fn frontmatter_parse_global_refused() {
        // The Wave 2 prune dropped `global`. Refuse loudly rather than
        // silently coercing — see ScopeError::Unsupported.
        assert!(matches!(
            Scope::from_frontmatter_str("global"),
            Err(ScopeError::Unsupported(_))
        ));
    }

    #[test]
    fn frontmatter_parse_user_refused() {
        assert!(matches!(
            Scope::from_frontmatter_str("user:alice"),
            Err(ScopeError::Unsupported(_))
        ));
    }

    #[test]
    fn frontmatter_parse_unknown_refused() {
        assert!(matches!(
            Scope::from_frontmatter_str("admin"),
            Err(ScopeError::Unsupported(_))
        ));
        assert!(matches!(
            Scope::from_frontmatter_str(""),
            Err(ScopeError::Unsupported(_))
        ));
    }

    #[test]
    fn bind_to_workspace_fills_empty_placeholder() {
        let unbound = Scope::Workspace {
            path: PathBuf::new(),
        };
        let bound = unbound.bind_to_workspace(Path::new("/k"));
        match bound {
            Scope::Workspace { path } => assert_eq!(path, PathBuf::from("/k")),
        }
    }

    #[test]
    fn bind_to_workspace_leaves_bound_scope_alone() {
        assert_eq!(ws("/a").bind_to_workspace(Path::new("/k")), ws("/a"));
    }

    #[test]
    fn property_value_roundtrip_workspace() {
        let s = ws("/x/y");
        let v = s.to_property_value();
        let decoded = Scope::from_property_value(&v).unwrap().unwrap();
        assert_eq!(decoded, s);
    }

    #[test]
    fn property_value_decodes_relaxed_string() {
        let v = serde_json::Value::String("workspace:/x".into());
        let decoded = Scope::from_property_value(&v).unwrap().unwrap();
        assert_eq!(decoded, ws("/x"));
    }

    #[test]
    fn property_value_legacy_global_refused() {
        let v = serde_json::json!({ "kind": "global" });
        let decoded = Scope::from_property_value(&v);
        assert!(matches!(decoded, Some(Err(ScopeError::Unsupported(_)))));
    }

    #[test]
    fn property_value_legacy_user_refused() {
        let v = serde_json::json!({ "kind": "user", "id": "alice" });
        let decoded = Scope::from_property_value(&v);
        assert!(matches!(decoded, Some(Err(ScopeError::Unsupported(_)))));
    }

    #[test]
    fn property_value_decodes_missing_as_none() {
        assert!(Scope::from_property_value(&serde_json::Value::Null).is_none());
    }

    #[test]
    fn display_workspace() {
        assert_eq!(ws("/foo").to_string(), "workspace:/foo");
    }
}
