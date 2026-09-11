//! Joining the asset table to the path: `search_paths`.

use super::asset::RuntimeAsset;
use super::entry::{EntryKind, Origin, RuntimeEntry, SearchPath};

/// Every candidate directory for `asset`, highest priority first.
///
/// # Candidates are returned UNFILTERED
///
/// This function never calls `exists()`, and callers must not assume it did.
/// The three existing resolvers disagree about existence *on purpose*:
///
/// - `runtime_plugin_paths` filters, and a test asserts a root without a
///   `plugins/` subdirectory is not offered.
/// - `defaults_candidates_from` deliberately does **not** filter. It records
///   candidates that do not exist, because the file an agent plants is by
///   definition the file that was not there, and the write-protected set is
///   built from what the resolvers hand out.
///
/// A shared resolver that filtered would break the second; one that filtered
/// nowhere and forced every caller to is what this is. Record first, then
/// filter.
///
/// # What is skipped
///
/// An entry whose [`super::entry::Origin`] the asset does not reach, and a
/// leaf belonging to a different asset. Both are containment, not tidiness:
/// the origin check is what keeps a cloned kiln from becoming a plugin root.
pub fn search_paths(asset: RuntimeAsset, path: &[RuntimeEntry]) -> Vec<SearchPath> {
    // Skills rank a kiln above a workspace; cards rank a workspace above a
    // kiln. See `RuntimeAsset::kiln_outranks_workspace` for why that is
    // preserved rather than unified.
    let mut ordered: Vec<&RuntimeEntry> = path.iter().collect();
    if asset.kiln_outranks_workspace() {
        ordered.sort_by_key(|e| match e.origin {
            Origin::Workspace => Origin::Kiln,
            Origin::Kiln => Origin::Workspace,
            other => other,
        });
    }

    ordered
        .into_iter()
        .filter(|entry| asset.reaches(entry.origin))
        .filter_map(|entry| {
            let dir = match &entry.kind {
                EntryKind::Root(root) => root.join(asset.subdir()),
                EntryKind::Leaf(leaf, owner) if *owner == asset => leaf.clone(),
                EntryKind::Leaf(_, _) => return None,
            };
            Some((dir, entry))
        })
        .enumerate()
        .map(|(rank, (path, entry))| SearchPath {
            path,
            origin: entry.origin,
            harness: entry.harness.clone(),
            rank,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_path::entry::Origin;
    use std::path::PathBuf;

    fn roots() -> Vec<RuntimeEntry> {
        vec![
            RuntimeEntry::root("/user", Origin::UserConfig),
            RuntimeEntry::root("/bundled", Origin::Bundled),
        ]
    }

    /// Root order is the returned order, for every kind that reaches both.
    ///
    /// `Defaults` refuses `UserConfig` — `defaults/` names Crucible's own
    /// defaults, and the user's entry point is `~/.config/crucible/init.lua`
    /// beside it — so it sees only the bundled root.
    #[test]
    fn root_order_is_precedence_for_every_asset() {
        for asset in RuntimeAsset::ALL {
            let found: Vec<PathBuf> = search_paths(asset, &roots())
                .into_iter()
                .map(|s| s.path)
                .collect();
            let expected: Vec<PathBuf> =
                [(Origin::UserConfig, "/user"), (Origin::Bundled, "/bundled")]
                    .iter()
                    .filter(|(origin, _)| asset.reaches(*origin))
                    .map(|(_, root)| PathBuf::from(root).join(asset.subdir()))
                    .collect();
            assert_eq!(found, expected, "{asset:?} did not preserve root order");
        }
    }

    /// A non-existent root is still returned.
    ///
    /// `defaults_candidates` depends on this: protection is judged on the
    /// name, because the file an agent plants is the one that did not exist.
    #[test]
    fn a_missing_root_is_still_a_candidate() {
        let found = search_paths(
            RuntimeAsset::Defaults,
            &[RuntimeEntry::root(
                "/definitely/not/here",
                Origin::Config(0),
            )],
        );
        assert_eq!(found.len(), 1, "existence must not be consulted here");
    }

    /// No executing kind resolves anything under a kiln or a workspace.
    #[test]
    fn an_executing_kind_resolves_nothing_from_a_kiln_or_workspace() {
        let path = vec![
            RuntimeEntry::root("/kiln/.crucible", Origin::Kiln),
            RuntimeEntry::root("/ws/.crucible", Origin::Workspace),
        ];
        for asset in RuntimeAsset::ALL.iter().filter(|a| a.executes()) {
            assert!(
                search_paths(*asset, &path).is_empty(),
                "{asset:?} reached a kiln or workspace root"
            );
        }
    }

    /// Skills and cards do resolve from a kiln and a workspace.
    #[test]
    fn text_kinds_resolve_from_a_kiln_and_a_workspace() {
        let path = vec![
            RuntimeEntry::root("/kiln/.crucible", Origin::Kiln),
            RuntimeEntry::root("/ws/.crucible", Origin::Workspace),
        ];
        for asset in [RuntimeAsset::Skills, RuntimeAsset::Cards] {
            assert_eq!(search_paths(asset, &path).len(), 2, "{asset:?}");
        }
    }

    /// A leaf serves its own kind and no other.
    #[test]
    fn a_leaf_serves_only_its_own_asset() {
        let path = vec![RuntimeEntry::leaf("/x", Origin::Env, RuntimeAsset::Plugins)];

        let plugins = search_paths(RuntimeAsset::Plugins, &path);
        assert_eq!(
            plugins.iter().map(|s| s.path.clone()).collect::<Vec<_>>(),
            vec![PathBuf::from("/x")],
            "CRUCIBLE_PLUGIN_PATH=/x must search /x, not /x/plugins"
        );

        assert!(
            search_paths(RuntimeAsset::Skills, &path).is_empty(),
            "a plugins leaf must not offer itself to skills"
        );
    }

    /// A plugin's own directory supplies skills but never plugins.
    #[test]
    fn a_plugin_root_supplies_skills_but_not_plugins() {
        let path = vec![RuntimeEntry::root("/p/my-plugin", Origin::Plugin)];
        assert_eq!(search_paths(RuntimeAsset::Skills, &path).len(), 1);
        assert!(
            search_paths(RuntimeAsset::Plugins, &path).is_empty(),
            "plugin discovery must not recurse into plugin roots"
        );
    }

    /// Rank is dense and ascending after skipped entries are dropped.
    #[test]
    fn rank_is_dense_after_skips() {
        let path = vec![
            RuntimeEntry::root("/kiln", Origin::Kiln),
            RuntimeEntry::root("/user", Origin::UserConfig),
            RuntimeEntry::root("/bundled", Origin::Bundled),
        ];
        let found = search_paths(RuntimeAsset::Plugins, &path);
        assert_eq!(
            found.iter().map(|s| s.rank).collect::<Vec<_>>(),
            vec![0, 1],
            "the kiln entry is skipped, so ranks must close up"
        );
    }
}
