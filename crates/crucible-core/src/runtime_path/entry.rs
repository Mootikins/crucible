//! The roots on the path, and where each came from.
//!
//! [`super::asset`] says what lives under a root. This says which roots there
//! are and in what order. Position in the list is precedence, for every kind —
//! replacing three different rules that ran in two different directions.

use std::path::PathBuf;

/// Where a root came from. **Declaration order is precedence order**, highest
/// first, and [`super::resolve::search_paths`] preserves it.
///
/// The ranking is not arbitrary. Anything the user named outranks anything
/// Crucible supplies, and a plugin's own contribution sits below every
/// user-named root so a plugin can never shadow what the user wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// `$CRUCIBLE_RTP`, `$CRUCIBLE_PLUGIN_PATH`. Highest, for dev and CI.
    Env,
    /// `<workspace>/<workspace_root>` — a project directory.
    Workspace,
    /// `<kiln>/.crucible`.
    Kiln,
    /// A `runtimepath` entry, by its index in that list.
    Config(usize),
    /// A `[harnesses]` row: another agent tool's directory the user named.
    Harness,
    /// `~/.config/crucible`.
    ///
    /// Distinct from [`Self::UserRuntime`]. Plugins, cards, skills and themes
    /// read here; `cru setup` writes to the *other* one. Collapsing the two is
    /// what made shipped themes invisible.
    UserConfig,
    /// `~/.config/crucible/runtime` — where `cru setup` copies the tree.
    UserRuntime,
    /// A loaded plugin's own directory.
    ///
    /// In Vim a plugin directory is a `runtimepath` entry that may carry
    /// `colors/` and `plugin/` of its own. This is that, and it is what lets a
    /// plugin ship skills, cards and themes. Ranked below every user-named
    /// root deliberately: a plugin contributes, it does not override.
    Plugin,
    /// Shipped: exe-relative, then the tree extracted from the binary.
    Bundled,
}

/// One root on the path.
///
/// Two existing knobs name a *leaf* directory rather than a root, so this is
/// an enum and not a bare `PathBuf`:
///
/// - `CRUCIBLE_PLUGIN_PATH=/x` searches `/x`, not `/x/plugins`
/// - `agent_directories` entries are card directories
///
/// A leaf serves exactly one asset kind and is skipped by every other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// `<path>/<subdir>/` for every kind that reaches this origin.
    Root(PathBuf),
    /// `<path>/` itself, for one kind only.
    Leaf(PathBuf, super::asset::RuntimeAsset),
}

/// A root, its provenance, and the harness it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEntry {
    pub kind: EntryKind,
    pub origin: Origin,
    /// The harness a `[harnesses]` root belongs to, for provenance in
    /// `cru skills list` and the web view. `None` for a root the user named
    /// directly.
    ///
    /// An open string, not an enum: adding a vendor must be a config row, not
    /// a code change. Render layers treat it as an opaque label.
    pub harness: Option<String>,
}

impl RuntimeEntry {
    /// A root serving every kind that reaches `origin`.
    pub fn root(path: impl Into<PathBuf>, origin: Origin) -> Self {
        Self {
            kind: EntryKind::Root(path.into()),
            origin,
            harness: None,
        }
    }

    /// A directory that *is* one kind's directory, not a root above it.
    pub fn leaf(
        path: impl Into<PathBuf>,
        origin: Origin,
        asset: super::asset::RuntimeAsset,
    ) -> Self {
        Self {
            kind: EntryKind::Leaf(path.into(), asset),
            origin,
            harness: None,
        }
    }

    /// Tag this entry with the harness it came from.
    pub fn with_harness(mut self, harness: impl Into<String>) -> Self {
        self.harness = Some(harness.into());
        self
    }

    /// The path this entry names, whatever its kind.
    pub fn path(&self) -> &std::path::Path {
        match &self.kind {
            EntryKind::Root(p) => p,
            EntryKind::Leaf(p, _) => p,
        }
    }
}

/// One directory a kind may be resolved from, with the entry it came from.
///
/// Carries the index so a caller that must report precedence — `cru doctor`,
/// the skills view — does not recompute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPath {
    pub path: PathBuf,
    pub origin: Origin,
    pub harness: Option<String>,
    /// Position in the resolved list. Lower wins.
    pub rank: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Origin ordering is precedence, and the two user roots are distinct.
    #[test]
    fn origin_order_ranks_user_roots_above_plugin_and_bundled() {
        assert!(Origin::Env < Origin::Workspace);
        assert!(Origin::Config(0) < Origin::Harness);
        assert!(Origin::UserConfig < Origin::UserRuntime);
        assert!(Origin::UserRuntime < Origin::Plugin);
        assert!(Origin::Plugin < Origin::Bundled);
    }

    /// A plugin never outranks a root the user named.
    #[test]
    fn a_plugin_root_never_outranks_a_user_named_root() {
        for user in [
            Origin::Env,
            Origin::Workspace,
            Origin::Kiln,
            Origin::Config(0),
            Origin::Harness,
            Origin::UserConfig,
            Origin::UserRuntime,
        ] {
            assert!(
                user < Origin::Plugin,
                "{user:?} must outrank a plugin's own directory"
            );
        }
    }

    #[test]
    fn path_reads_through_either_kind() {
        let root = RuntimeEntry::root("/a", Origin::UserConfig);
        let leaf = RuntimeEntry::leaf("/b", Origin::Env, super::super::asset::RuntimeAsset::Cards);
        assert_eq!(root.path(), std::path::Path::new("/a"));
        assert_eq!(leaf.path(), std::path::Path::new("/b"));
    }
}
