//! Joining the asset table to the path: `search_paths`.

use super::asset::RuntimeAsset;
use super::entry::{EntryKind, RuntimeEntry, SearchPath};
use crate::sources::{sources_new, Source, Sources};

/// Every candidate directory for `asset`, in path order. The order is not
/// precedence: [`search_sources`] sorts by priority.
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
    candidates(asset, path)
        .into_iter()
        .map(|source| source.value)
        .collect()
}

/// Every candidate directory for `asset` as a source, sorted by priority.
///
/// The same filter as [`search_paths`]: an origin the asset does not reach
/// and a leaf of another asset are skipped, so a priority never gives an
/// origin an asset that [`RuntimeAsset::reaches`] refuses. The order comes
/// from each entry's priority, not from its position. Nothing here reads
/// the filesystem.
pub fn search_sources(
    asset: RuntimeAsset,
    path: &[RuntimeEntry],
) -> anyhow::Result<Sources<SearchPath>> {
    let (kept, clashes) = drop_repeated_sources(candidates(asset, path));
    for clash in &clashes {
        tracing::warn!(
            source = %clash.name,
            skipped = %clash.value.path.display(),
            "two sources have one name; the lower one is skipped"
        );
    }
    sources_new(kept)
}

/// The sources of `asset` that [`search_sources`] skips because a higher
/// source has the same name. `cru doctor` reports each one as an error.
pub fn name_clashes(asset: RuntimeAsset, path: &[RuntimeEntry]) -> Vec<Source<SearchPath>> {
    drop_repeated_sources(candidates(asset, path)).1
}

fn candidates(asset: RuntimeAsset, path: &[RuntimeEntry]) -> Vec<Source<SearchPath>> {
    path.iter()
        .filter(|entry| asset.reaches(entry.origin))
        .filter_map(|entry| {
            let dir = match &entry.kind {
                EntryKind::Root(root) => root.join(asset.subdir()),
                EntryKind::Leaf(leaf, owner) if *owner == asset => leaf.clone(),
                EntryKind::Leaf(_, _) => return None,
            };
            Some((dir, entry))
        })
        .map(|(dir, entry)| Source {
            name: entry.name.clone(),
            priority: entry.priority,
            within: entry.within,
            value: SearchPath {
                path: dir,
                origin: entry.origin,
                harness: entry.harness.clone(),
            },
        })
        .collect()
}

/// Keep one source per directory and one per name: the higher one. The
/// second list holds the sources dropped for a name clash.
///
/// A kiln that is also the workspace, or `~/.config/crucible` on
/// `runtimepath`, would otherwise offer each entry twice. A plugin or a kiln
/// takes its own name as its source name, so it can clash with another
/// source; the lower source is skipped. The paths are compared as written;
/// nothing here reads the filesystem.
fn drop_repeated_sources(
    list: Vec<Source<SearchPath>>,
) -> (Vec<Source<SearchPath>>, Vec<Source<SearchPath>>) {
    let rank = |i: usize, s: &Source<SearchPath>| (std::cmp::Reverse(s.priority), s.within, i);
    // `Some(true)`: keep. `Some(false)`: same directory. `None`: name clash.
    let verdicts: Vec<Option<bool>> = list
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let higher = list
                .iter()
                .enumerate()
                .filter(|(j, other)| rank(*j, other) < rank(i, source));
            for (_, other) in higher {
                if other.value.path == source.value.path {
                    return Some(false);
                }
                if other.name == source.name {
                    return None;
                }
            }
            Some(true)
        })
        .collect();
    let mut kept = Vec::new();
    let mut clashes = Vec::new();
    for (source, verdict) in list.into_iter().zip(verdicts) {
        match verdict {
            Some(true) => kept.push(source),
            Some(false) => {}
            None => clashes.push(source),
        }
    }
    (kept, clashes)
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

    /// The sources of an asset sort by priority and skip what the asset does
    /// not reach.
    #[test]
    fn search_sources_sort_by_priority_and_keep_containment() {
        let path = vec![
            RuntimeEntry::root("/bundled", Origin::Bundled),
            RuntimeEntry::root("/kiln/.crucible", Origin::Kiln),
            RuntimeEntry::root("/user", Origin::UserConfig),
        ];
        let themes = search_sources(RuntimeAsset::Themes, &path).unwrap();
        let names: Vec<&str> = themes.list().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["personal", "builtin"]);
        assert_eq!(themes.list()[0].value.path, PathBuf::from("/user/themes"));
    }

    /// A directory on the path twice is one source, the higher one.
    #[test]
    fn a_directory_on_the_path_twice_is_one_source() {
        let path = vec![
            RuntimeEntry::root("/k/.crucible", Origin::Kiln),
            RuntimeEntry::root("/k/.crucible", Origin::Workspace),
        ];
        let skills = search_sources(RuntimeAsset::Skills, &path).unwrap();
        let names: Vec<&str> = skills.list().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["workspace"]);
    }

    /// A plugin named like another source is skipped, not an error: the
    /// other sources still resolve.
    #[test]
    fn a_lower_source_with_a_taken_name_is_skipped() {
        let mut plugin = RuntimeEntry::root("/p/personal", Origin::Plugin);
        plugin.name = "personal".to_string();
        let path = vec![RuntimeEntry::root("/user", Origin::UserConfig), plugin];
        let skills = search_sources(RuntimeAsset::Skills, &path).unwrap();
        let dirs: Vec<PathBuf> = skills.list().iter().map(|s| s.value.path.clone()).collect();
        assert_eq!(dirs, [PathBuf::from("/user/skills")]);
        let clashes = name_clashes(RuntimeAsset::Skills, &path);
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0].value.path, PathBuf::from("/p/personal/skills"));
    }
}
