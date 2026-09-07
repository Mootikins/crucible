//! The asset kinds a runtime root can hold.
//!
//! Crucible resolved five kinds of on-disk asset through five resolvers, each
//! with its own root list and its own precedence rule. Plugins took the first
//! match over a highest-first list; agent cards took the last match over a
//! lowest-first list; skills sorted by a scope enum and then took the last of
//! an equal-scope run. A reader could not carry one rule to the next file, and
//! the drift shipped bugs: `cru agents list` advertised cards the daemon would
//! not resolve, and an installed `cru` found none of its bundled plugins.
//!
//! This is the [`crate::runtime_path`] half that says WHAT lives under a root.
//! [`super::entry`] says WHICH roots there are, and `resolve` joins the two.
//!
//! # Why an enum and not a table of strings
//!
//! Adding a name to a closed set needs one enumerated table with a real gate,
//! and the exemplar is `crucible-daemon/src/tools/surface.rs`. Every method
//! below is a total function with no wildcard arm, so a new variant does not
//! compile until someone answers all four questions for it. The two
//! module-level denies stop a wildcard being used to silence that; both are
//! needed, because clippy reports a wildcard covering one remaining variant
//! under a different lint than one covering two or more.
//!
//! There is deliberately no `Default` on [`RuntimeAsset`] or [`EntryShape`].
//! A default would let "nobody classified this" mean something, and that is
//! how the four hand-maintained lists this replaces each became satisfiable
//! without the entry they were meant to require.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use super::entry::Origin;

/// A kind of asset that lives under a runtime root, at a well-known
/// subdirectory.
///
/// Vim's model: `runtimepath` is one ordered list of roots, and each kind is a
/// subdirectory of every entry. Crucible had the list and never generalised
/// the subdirectory rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum RuntimeAsset {
    /// `<root>/plugins/<name>/` — Lua the daemon executes.
    Plugins,
    /// `<root>/skills/<name>/SKILL.md` — text that reaches a system prompt.
    Skills,
    /// `<root>/agents/<name>.md` — agent cards.
    Cards,
    /// `<root>/themes/<name>.luau` — Lua the theme VM executes.
    Themes,
    /// `<root>/defaults/init.luau` — the daemon VM's own defaults.
    ///
    /// One fixed file chosen first-hit, with no name space, so `subdir` and
    /// `shape` are near-trivial for it. It stays in this enum anyway: the
    /// kiln invariant `execution_roots` rests on covers plugins *and*
    /// defaults, and a hand-written origin filter in `runtime_defaults.rs`
    /// would leave the defaults half proved by nothing.
    Defaults,
}

/// How an entry is recognised inside `<root>/<subdir>/`.
///
/// # These name predicate families, not extensions
///
/// An earlier draft spelled the extensions out — `FilesInDir(["md"])` for
/// cards, `["luau", "lua"]` for themes. Both were wrong, and one was caught by
/// `nobody_hand_rolls_the_markdown_extension_check`:
///
/// - Agent cards already go through `crucible_core::kiln::is_note_file`
///   (`agent/loader.rs:39`), which accepts `md` **and** `markdown`, lowercased.
///   A literal `["md"]` silently understated what the loader finds.
/// - `crucible_lua::source_files` exists because "sites across three crates
///   used to decide *is this a Lua file*, each with its own literal". This
///   crate cannot call it — `crucible-core` does not depend on `crucible-lua`,
///   and must not — so naming the family is the only honest option.
///
/// So no extension literal appears below. Each variant names *who decides*,
/// and the consumer resolves it in a crate that can reach that answer.
/// `SKILL.md` and `plugin.yaml` stay literal because nothing else owns them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryShape {
    /// A directory holding this marker file. Skills: `SKILL.md`.
    DirWithMarker(&'static str),
    /// A plugin directory: a `plugin.yaml`, **or** a Lua entry file.
    ///
    /// Bespoke because the rule is: either may identify a plugin, and a
    /// directory holding both entry-file spellings is refused rather than
    /// resolved (`crucible_lua::source_files::init_file`).
    PluginDir,
    /// Markdown files directly in the subdirectory. `is_note_file` decides.
    /// The entry name is the file stem. Cards.
    NoteFiles,
    /// Lua source files directly in the subdirectory. `source_files` decides.
    /// The entry name is the file stem. Themes.
    LuaSourceFiles,
    /// The one Lua entry file. `source_files::init_file_names` decides, and
    /// the first that exists wins. Defaults.
    LuaEntryFile,
}

impl RuntimeAsset {
    /// Every variant. Completeness is proved by walking `strum::EnumIter` in
    /// this module's tests rather than by a second hand-written list.
    pub const ALL: [RuntimeAsset; 5] = [
        RuntimeAsset::Plugins,
        RuntimeAsset::Skills,
        RuntimeAsset::Cards,
        RuntimeAsset::Themes,
        RuntimeAsset::Defaults,
    ];

    /// The subdirectory of a root this kind lives in.
    pub fn subdir(self) -> &'static str {
        match self {
            RuntimeAsset::Plugins => "plugins",
            RuntimeAsset::Skills => "skills",
            RuntimeAsset::Cards => "agents",
            RuntimeAsset::Themes => "themes",
            RuntimeAsset::Defaults => "defaults",
        }
    }

    /// How to recognise one entry.
    ///
    /// Plugins accept a manifest **or** a bare entry file; a directory holding
    /// both entry-file spellings is an error the loader reports, not a
    /// silently dropped plugin.
    pub fn shape(self) -> EntryShape {
        match self {
            RuntimeAsset::Plugins => EntryShape::PluginDir,
            RuntimeAsset::Skills => EntryShape::DirWithMarker("SKILL.md"),
            RuntimeAsset::Cards => EntryShape::NoteFiles,
            RuntimeAsset::Themes => EntryShape::LuaSourceFiles,
            RuntimeAsset::Defaults => EntryShape::LuaEntryFile,
        }
    }

    /// Whether the daemon loads or executes code from this kind.
    ///
    /// Gates `execution_roots::record`. It answers a different question from
    /// [`Self::reaches`]: recording stops the *agent* from planting a file in
    /// a tree the daemon runs, and says nothing about which roots a kind may
    /// see in the first place.
    ///
    /// Skills and cards are false. They are text, and recording them would
    /// write-protect the very directories a user expects an agent to edit.
    pub fn executes(self) -> bool {
        match self {
            RuntimeAsset::Plugins | RuntimeAsset::Themes | RuntimeAsset::Defaults => true,
            RuntimeAsset::Skills | RuntimeAsset::Cards => false,
        }
    }

    /// Whether this kind may be resolved from a root of `origin`.
    ///
    /// **This is the containment half, and it is not the same as
    /// [`Self::executes`].** A kiln reaches the loaders through exactly one
    /// route: the user names it on the config `runtimepath`. Nothing a kiln
    /// *contains* puts it there, because a kiln is cloned or synced from
    /// elsewhere and attaching one must not mean executing what it ships.
    /// Write protection answers a different question — it stops the agent
    /// planting a plugin, not a person shipping one inside a kiln someone else
    /// authored. So the executing kinds refuse `Workspace` and `Kiln` here,
    /// and `execution_roots`' own gate stays green.
    ///
    /// `Plugins` also refuses [`Origin::Plugin`]: a plugin does not ship
    /// plugins, and allowing it would make discovery recursive.
    pub fn reaches(self, origin: Origin) -> bool {
        match self {
            RuntimeAsset::Plugins => {
                !matches!(origin, Origin::Workspace | Origin::Kiln | Origin::Plugin)
            }
            RuntimeAsset::Themes | RuntimeAsset::Defaults => {
                !matches!(origin, Origin::Workspace | Origin::Kiln)
            }
            RuntimeAsset::Skills | RuntimeAsset::Cards => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// `ALL` covers every variant, derived from what the compiler knows.
    #[test]
    fn all_covers_every_variant() {
        let iter: Vec<RuntimeAsset> = RuntimeAsset::iter().collect();
        assert_eq!(
            iter.len(),
            RuntimeAsset::ALL.len(),
            "ALL is stale: {iter:?} vs {:?}",
            RuntimeAsset::ALL
        );
        for asset in RuntimeAsset::iter() {
            assert!(
                RuntimeAsset::ALL.contains(&asset),
                "{asset:?} is missing from ALL"
            );
        }
    }

    /// Every kind names a distinct, non-empty subdirectory.
    ///
    /// Distinctness matters: two kinds sharing a subdirectory would make one
    /// silently resolve the other's entries.
    #[test]
    fn every_asset_names_a_distinct_subdir() {
        let mut seen = std::collections::BTreeSet::new();
        for asset in RuntimeAsset::iter() {
            let subdir = asset.subdir();
            assert!(!subdir.is_empty(), "{asset:?} has no subdir");
            assert!(
                seen.insert(subdir),
                "{asset:?} reuses the subdir {subdir:?}"
            );
        }
    }

    /// Every kind gets its own shape, and no shape spells out an extension
    /// another crate owns.
    ///
    /// The second half is the load-bearing one. A literal `["md"]` for cards
    /// understated `is_note_file`, which takes `md` AND `markdown` in any
    /// case, and `nobody_hand_rolls_the_markdown_extension_check` caught it.
    /// The Lua equivalent is not gated anywhere, so it is gated here.
    #[test]
    fn no_shape_hard_codes_an_extension_another_crate_owns() {
        for asset in RuntimeAsset::iter() {
            match asset.shape() {
                // Owned by nothing else: the skills spec names SKILL.md.
                EntryShape::DirWithMarker(marker) => {
                    assert!(!marker.is_empty(), "{asset:?} has an empty marker");
                    assert!(
                        !marker.eq_ignore_ascii_case("markdown"),
                        "{asset:?} names a markdown extension; ask is_note_file"
                    );
                }
                // These four name WHO decides, and carry no extension at all.
                EntryShape::PluginDir
                | EntryShape::NoteFiles
                | EntryShape::LuaSourceFiles
                | EntryShape::LuaEntryFile => {}
            }
        }
    }

    /// Cards are recognised by the note predicate, not by a literal.
    ///
    /// `agent/loader.rs` already calls `is_note_file`, so a table that said
    /// `["md"]` would describe a loader that does not exist.
    #[test]
    fn cards_defer_to_the_note_predicate() {
        assert_eq!(RuntimeAsset::Cards.shape(), EntryShape::NoteFiles);
    }

    /// No executing kind accepts a workspace or a kiln root.
    ///
    /// This is `a_kiln_off_the_runtimepath_contributes_no_execution_root`
    /// stated at the type level. That test proves the resolvers behave; this
    /// one proves the table they will be built from cannot say otherwise.
    #[test]
    fn an_executing_kind_never_reaches_a_workspace_or_kiln() {
        for asset in RuntimeAsset::iter().filter(|a| a.executes()) {
            assert!(
                !asset.reaches(Origin::Workspace),
                "{asset:?} executes and would run code from a workspace"
            );
            assert!(
                !asset.reaches(Origin::Kiln),
                "{asset:?} executes and would run code from a cloned kiln"
            );
        }
    }

    /// Plugin discovery cannot recurse: no plugin root feeds plugin lookup.
    #[test]
    fn plugins_never_reach_a_plugin_root() {
        assert!(!RuntimeAsset::Plugins.reaches(Origin::Plugin));
    }

    /// A plugin may ship the three non-executing-lookup kinds.
    ///
    /// This is what lets `crucible-help` keep its skills beside its manifest
    /// instead of them living under a special-cased glob.
    #[test]
    fn a_plugin_root_supplies_skills_cards_and_themes() {
        for asset in [
            RuntimeAsset::Skills,
            RuntimeAsset::Cards,
            RuntimeAsset::Themes,
        ] {
            assert!(
                asset.reaches(Origin::Plugin),
                "{asset:?} should resolve from a plugin's own directory"
            );
        }
    }

    /// Text kinds are never recorded as execution roots.
    ///
    /// Recording `~/.claude/skills` would write-protect a directory the user
    /// expects an agent to edit.
    #[test]
    fn text_kinds_do_not_execute() {
        assert!(!RuntimeAsset::Skills.executes());
        assert!(!RuntimeAsset::Cards.executes());
    }
}
