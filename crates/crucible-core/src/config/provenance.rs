//! Per-leaf provenance for the config store.
//!
//! [`ProvenanceMap`] records, for each dot-joined leaf path, which source
//! last wrote it. An array is one leaf: arrays replace wholesale, so
//! per-element provenance cannot exist.
//!
//! [`SourceTag`] is a closed set, and every reading of it is a total function
//! of it: its two renderings, its [`SourceTag::rank`], its
//! [`SourceTag::origin`] and the refusal rule behind [`SourceTag::pin`]. A
//! wildcard arm would let a new layer inherit another layer's name — and,
//! worse, another layer's rank, which decides who wins a contested leaf, or
//! another layer's answer to whether `config.save` may write it. The two module-level denies below catch
//! that under `cargo clippy`, which `just ci` runs and `cargo test` does not.
//! Both are needed: clippy reports a wildcard that covers ONE remaining
//! variant as `match_wildcard_for_single_variants`, and only a wildcard that
//! covers two or more as `wildcard_enum_match_arm`. Layers arrive one at a
//! time, so the single-variant lint is the one that fires.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a config value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum SourceTag {
    /// The compiled-in default.
    Default,
    /// A plugin's own `setup()` write, made while `init.lua` evaluates.
    ///
    /// The caller decides this layer, not the phase: a plugin file writes a
    /// default even when the user's `init.lua` is the line that called it.
    PluginDefault {
        /// The plugin that owns the file that wrote.
        plugin: String,
        /// The Lua chunk name: the plugin file's path.
        file: String,
        /// The call-site line; `None` for a returned table.
        line: Option<u32>,
    },
    /// The machine-written `settings.json`, which the settings UI saves.
    Settings,
    /// The deprecated `config.toml` seed.
    Toml(std::path::PathBuf),
    /// A `cru.config.set` call (or the returned table) in a Lua file the
    /// human owns.
    Lua {
        /// The Lua chunk name, usually the file path.
        file: String,
        /// The call-site line; `None` for a returned table.
        line: Option<u32>,
    },
    /// The daemon state overlay (`kilns.json`, `llm.json`) — what the daemon
    /// was told through a registration surface.
    Registered,
    /// A CLI flag override.
    Cli,
    /// A runtime `config.set` RPC merge.
    Rpc,
}

/// Where one leaf came from, in the shape the wire uses.
///
/// One projection serves two callers: `config.origin` answers with it, and a
/// `config.save` refusal names the pin with it. Two projections would let the
/// refusal name a file the origin does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceOrigin {
    /// The one-word source name, as [`SourceTag::short`] gives it.
    pub source: &'static str,
    /// The file the source names, when it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The line inside `file`, when the source recorded one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Where one leaf came from, and whether `config.save` would refuse it.
///
/// The row `config.origin` answers with. `pinned` is the refusal rule itself,
/// reported per leaf, and the [`SourceOrigin`] beside it is the pin that
/// refuses — the same one the refusal carries. See [`crate::config::ConfigStore::origin`]
/// for why the pin, and not the last writer, names the file here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeafOrigin {
    /// Whether a boot-restored layer holds this leaf, so `config.save`
    /// refuses it.
    pub pinned: bool,
    /// The source that owns the leaf, with its file and line when it has
    /// them.
    #[serde(flatten)]
    pub origin: SourceOrigin,
}

impl SourceTag {
    /// The layer this source belongs to, lowest precedence first.
    ///
    /// The rank answers one question: when two sources write one leaf, whose
    /// value does the store keep? It lives in one method because the same
    /// order is read by the merge, by the refusal that protects a human's
    /// line, and by the settings UI. The same comparison written at each call
    /// site becomes as many orders as there are sites, and they disagree.
    ///
    /// The order, and the reason for each step:
    ///
    /// 1. `Default` is the compiled-in value. Every author overrides it.
    /// 2. `PluginDefault` is a plugin's `setup()` write. A plugin runs during
    ///    the evaluation of `init.lua`, but it does not speak for the user,
    ///    so it must lose to the settings UI. If it did not, nearly every key
    ///    would lock and that UI would be useless.
    /// 3. `Settings` is `settings.json`: what the user saved through a UI.
    /// 4. `Toml` is the deprecated `config.toml` seed, which `cru config
    ///    migrate` moves into `init.lua`. Lua therefore outranks it.
    /// 5. `Lua` is a human's own line. It beats every persisted layer,
    ///    because a machine write that silently replaced it would act
    ///    nowhere: the human's file re-applies at the next boot.
    /// 6. `Registered` is what an operator told the daemon after the file was
    ///    written (`cru kiln register`), so it is later news than the file.
    /// 7. `Cli` is a flag the operator typed for this invocation.
    /// 8. `Rpc` is highest, and it is highest BECAUSE it is the weakest: it
    ///    is the ephemeral runtime knob (`:set`), it writes no file, and it
    ///    dies with the process. It may therefore override anything for one
    ///    run without taking authorship away from anybody.
    pub fn rank(&self) -> u8 {
        match self {
            SourceTag::Default => 0,
            SourceTag::PluginDefault { .. } => 1,
            SourceTag::Settings => 2,
            SourceTag::Toml(_) => 3,
            SourceTag::Lua { .. } => 4,
            SourceTag::Registered => 5,
            SourceTag::Cli => 6,
            SourceTag::Rpc => 7,
        }
    }

    /// Whether this source writes the leaf again at the next boot, from a
    /// layer that loads above `settings.json`.
    ///
    /// This is the whole refusal rule of `config.save`. A saved value lands in
    /// the `Settings` layer; if a higher layer restores the leaf at the next
    /// boot, the saved value acts nowhere, so the save is refused rather than
    /// lost.
    fn pins_a_leaf(&self) -> bool {
        match self {
            // Both load below `Settings`, so a save wins over them and acts.
            // The plugin layer is below on purpose: plugins run while
            // `init.lua` evaluates, and if their writes pinned, nearly every
            // key would lock and the settings UI would be useless.
            SourceTag::Default | SourceTag::PluginDefault { .. } => false,
            // The layer `config.save` writes. It cannot pin against itself.
            SourceTag::Settings => false,
            // Two files a person owns. Both re-apply at every boot.
            SourceTag::Toml(_) | SourceTag::Lua { .. } => true,
            // Daemon state (`kilns.json`, `llm.json`). It reloads at the next
            // boot, and a `settings.json` entry that half-described the same
            // provider would shadow the working one, so a save under it is
            // worse than lost. It names no file a person edits; `cru kiln
            // register` and the provider surfaces are the route to change it.
            SourceTag::Registered => true,
            // A flag typed for one invocation, and the runtime knob `:set`
            // writes. Neither survives the process, so neither re-applies.
            SourceTag::Cli | SourceTag::Rpc => false,
        }
    }

    /// Whether `config.reset` (`:set key&`) drops this layer's hold on a
    /// leaf.
    ///
    /// `&` undoes the runtime knob, so it drops exactly the layer that knob
    /// writes and nothing else. Every other layer is restored by a file or by
    /// the invocation, and dropping one in memory would answer with a value
    /// the next boot takes straight back.
    ///
    /// `Settings` is the interesting no. It is a durable file, so a reset
    /// that dropped it in memory would report a value that reverts, and a
    /// reset that deleted the leaf from the file would let a one-key undo of
    /// a session tweak destroy a preference the user saved through the
    /// settings UI. `config.save` writes that layer, and `config.save` is the
    /// verb that unwrites it. `config.pop` (`:set key^`) is the door for
    /// looking under it for one run.
    pub fn reset_drops(&self) -> bool {
        match self {
            // The compiled default and a plugin's declared default are what
            // `&` returns TO, so it cannot drop them.
            SourceTag::Default | SourceTag::PluginDefault { .. } => false,
            // The durable machine layer. See the doc comment above.
            SourceTag::Settings => false,
            // Two files a person owns. Both re-apply at every boot.
            SourceTag::Toml(_) | SourceTag::Lua { .. } => false,
            // Daemon state (`kilns.json`, `llm.json`). It reloads at the next
            // boot, and `cru kiln register` is the route to change it.
            SourceTag::Registered => false,
            // A flag the operator typed for this invocation. It dies with the
            // process, but it is the terms the daemon was started on, and a
            // keystroke in one client must not erase them for every client.
            SourceTag::Cli => false,
            // The ephemeral runtime knob `:set` writes: the one layer a
            // running session authors, and the one `&` undoes.
            SourceTag::Rpc => true,
        }
    }

    /// Where the leaf came from, for `config.origin`.
    pub fn origin(&self) -> SourceOrigin {
        let (file, line) = match self {
            SourceTag::PluginDefault { file, line, .. } | SourceTag::Lua { file, line } => {
                (Some(file.clone()), *line)
            }
            SourceTag::Toml(path) => (Some(path.display().to_string()), None),
            SourceTag::Default
            | SourceTag::Settings
            | SourceTag::Registered
            | SourceTag::Cli
            | SourceTag::Rpc => (None, None),
        };
        SourceOrigin {
            source: self.short(),
            file,
            line,
        }
    }

    /// The pin this source puts on a leaf, if it puts one.
    ///
    /// `config.save` refuses a pinned leaf and answers with this, so the user
    /// is told which line to change instead.
    pub fn pin(&self) -> Option<SourceOrigin> {
        self.pins_a_leaf().then(|| self.origin())
    }

    /// One-word source name for table rendering.
    pub fn short(&self) -> &'static str {
        match self {
            SourceTag::Default => "default",
            SourceTag::PluginDefault { .. } => "plugin",
            SourceTag::Settings => "settings",
            SourceTag::Toml(_) => "toml",
            SourceTag::Lua { .. } => "lua",
            SourceTag::Registered => "registered",
            SourceTag::Cli => "cli",
            SourceTag::Rpc => "rpc",
        }
    }

    /// The rendered detail, for `cru config show --sources`.
    pub fn detail(&self) -> String {
        match self {
            SourceTag::Default => "default".to_string(),
            SourceTag::PluginDefault {
                plugin,
                file,
                line: Some(line),
            } => format!("plugin {plugin} ({file}:{line})"),
            SourceTag::PluginDefault {
                plugin,
                file,
                line: None,
            } => format!("plugin {plugin} ({file})"),
            SourceTag::Settings => "settings".to_string(),
            SourceTag::Toml(path) => format!("toml ({})", path.display()),
            SourceTag::Lua {
                file,
                line: Some(line),
            } => format!("lua ({file}:{line})"),
            SourceTag::Lua { file, line: None } => format!("lua ({file})"),
            SourceTag::Registered => "registered".to_string(),
            SourceTag::Cli => "cli".to_string(),
            SourceTag::Rpc => "rpc".to_string(),
        }
    }
}

/// Dot-joined leaf path → the source that last wrote it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvenanceMap {
    entries: BTreeMap<String, SourceTag>,
}

impl ProvenanceMap {
    /// An empty map. Every absent entry renders as `default`.
    pub fn new() -> Self {
        Self::default()
    }

    /// The source recorded for `path`, if any.
    pub fn get(&self, path: &str) -> Option<&SourceTag> {
        self.entries.get(path)
    }

    /// Record `tag` for one leaf path.
    pub fn set(&mut self, path: impl Into<String>, tag: SourceTag) {
        self.entries.insert(path.into(), tag);
    }

    /// The highest rank recorded at `prefix` or under it, if anything is.
    ///
    /// A merge asks this before it writes, and asks it about the WHOLE
    /// subtree the write covers: an inserted or replaced branch lands over
    /// every leaf under it at once, while the branch itself carries no row of
    /// its own. Reading only the row at `prefix` would make `__replace` the
    /// door around the layer order.
    pub fn max_rank_at_or_under(&self, prefix: &str) -> Option<u8> {
        let child_prefix = format!("{prefix}.");
        self.entries
            .iter()
            .filter(|(path, _)| path.as_str() == prefix || path.starts_with(&child_prefix))
            .map(|(_, tag)| tag.rank())
            .max()
    }

    /// Drop every entry at `prefix` or under it.
    ///
    /// A replacement clears first, then records the new leaves — otherwise a
    /// removed provider would keep a ghost provenance row.
    pub fn clear_prefix(&mut self, prefix: &str) {
        let child_prefix = format!("{prefix}.");
        self.entries
            .retain(|path, _| path != prefix && !path.starts_with(&child_prefix));
    }

    /// Iterate the recorded entries in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SourceTag)> {
        self.entries.iter()
    }

    /// The number of recorded leaves.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// The one place the layer order is written down as a list. `rank` is the
    /// only definition; this test reads it back and states it in words, so a
    /// reordering is a visible diff rather than a changed integer.
    #[test]
    fn the_layer_order_runs_from_the_compiled_default_to_the_runtime_knob() {
        let mut sorted: Vec<SourceTag> = SourceTag::iter().collect();
        sorted.sort_by_key(SourceTag::rank);
        let order: Vec<&str> = sorted.iter().map(SourceTag::short).collect();
        assert_eq!(
            order,
            vec![
                "default",
                "plugin",
                "settings",
                "toml",
                "lua",
                "registered",
                "cli",
                "rpc",
            ]
        );
    }

    /// Two layers that share a rank cannot be ordered, so the store would keep
    /// whichever wrote last. `EnumIter` walks what the compiler knows, rather
    /// than a hand-written list that a new variant leaves behind.
    #[test]
    fn every_variant_holds_its_own_rank() {
        let mut ranks: Vec<u8> = SourceTag::iter().map(|tag| tag.rank()).collect();
        let count = ranks.len();
        ranks.sort_unstable();
        ranks.dedup();
        assert_eq!(ranks.len(), count, "two SourceTag variants share a rank");
        assert_eq!(
            ranks,
            (0..u8::try_from(count).expect("the layers fit in a u8")).collect::<Vec<u8>>(),
            "the ranks must run 0..n with no gap"
        );
    }

    /// `short` names the source in a table and in `source_short` on the wire.
    /// Two variants that share a name are indistinguishable there.
    #[test]
    fn every_variant_holds_its_own_short_name() {
        let mut names: Vec<&str> = SourceTag::iter().map(|tag| tag.short()).collect();
        let count = names.len();
        assert!(names.iter().all(|name| !name.is_empty()));
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            count,
            "two SourceTag variants share a short name"
        );
    }

    /// `config.reset` (`:set key&`) drops exactly one layer, and that layer
    /// is restored by no file.
    ///
    /// Both halves matter. Two droppable layers would make `&` an undo of
    /// something the user never asked to undo, and a layer with a file behind
    /// it would come back at the next boot, so the reset would report a value
    /// that reverts. Which layer it is comes from the other half of this gate,
    /// in `crucible-lua`: whatever `config.set` actually writes must be the
    /// one `reset_drops` names.
    #[test]
    fn a_reset_drops_exactly_one_layer_and_no_file_restores_it() {
        let droppable: Vec<SourceTag> = SourceTag::iter().filter(SourceTag::reset_drops).collect();
        assert_eq!(
            droppable.len(),
            1,
            "`&` undoes the runtime knob and nothing else, but {droppable:?} are droppable"
        );
        let tag = &droppable[0];
        assert_eq!(
            tag.pin(),
            None,
            "'{}' re-applies at the next boot, so a reset of it answers with a value \
             the next boot takes back",
            tag.short()
        );
        assert_eq!(
            tag.origin().file,
            None,
            "'{}' names a file, so dropping it in memory hides what that file still says",
            tag.short()
        );
    }

    /// A plugin's `setup()` write must lose to the settings UI, and a human's
    /// own line must beat it. This is the whole point of the two new layers.
    #[test]
    fn a_plugin_default_loses_to_settings_and_a_human_line_beats_it() {
        let plugin = SourceTag::PluginDefault {
            plugin: "alpha".to_string(),
            file: "/plugins/alpha/init.lua".to_string(),
            line: Some(4),
        };
        let human = SourceTag::Lua {
            file: "/config/init.lua".to_string(),
            line: Some(4),
        };
        assert!(plugin.rank() < SourceTag::Settings.rank());
        assert!(human.rank() > SourceTag::Settings.rank());
    }

    /// The refusal rule, checked against the rank rather than restated. A
    /// source that pins must load above `Settings`, or the saved value would
    /// have acted and the refusal took a working write away from the user.
    #[test]
    fn every_source_that_pins_a_leaf_loads_above_the_settings_layer() {
        for tag in SourceTag::iter() {
            if tag.pin().is_some() {
                assert!(
                    tag.rank() > SourceTag::Settings.rank(),
                    "'{}' pins a leaf but loads at or below settings, so a save \
                     would have taken effect",
                    tag.short()
                );
            }
        }
    }

    /// The two layers a `config.save` must be able to write over. A plugin's
    /// declared default is the load-bearing one: if it pinned, nearly every
    /// key would lock and the settings UI would refuse almost every save.
    #[test]
    fn a_plugin_default_and_the_settings_layer_pin_nothing() {
        let plugin = SourceTag::PluginDefault {
            plugin: "alpha".to_string(),
            file: "/plugins/alpha/init.lua".to_string(),
            line: Some(4),
        };
        assert_eq!(plugin.pin(), None);
        assert_eq!(SourceTag::Settings.pin(), None);
        assert_eq!(SourceTag::Default.pin(), None);
    }

    /// `:set` is the ephemeral knob. It must not pin, or one `:set` would
    /// lock the settings UI out of that key for the rest of the process.
    #[test]
    fn the_runtime_knob_pins_nothing() {
        assert_eq!(SourceTag::Rpc.pin(), None);
        assert_eq!(SourceTag::Cli.pin(), None);
    }

    /// A refused save names the line to change, so the pin carries the file
    /// and the line the human wrote.
    #[test]
    fn a_human_lua_line_pins_with_its_file_and_line() {
        let pin = SourceTag::Lua {
            file: "/home/u/.config/crucible/init.lua".to_string(),
            line: Some(12),
        }
        .pin()
        .expect("a human's own line pins");
        assert_eq!(pin.source, "lua");
        assert_eq!(
            pin.file.as_deref(),
            Some("/home/u/.config/crucible/init.lua")
        );
        assert_eq!(pin.line, Some(12));
    }

    /// `config.origin` answers with the source of every variant, so the
    /// projection must name each one. `EnumIter` walks what the compiler
    /// knows, so a new layer cannot slip through without a name.
    #[test]
    fn every_source_projects_its_own_name_on_the_wire() {
        for tag in SourceTag::iter() {
            assert_eq!(
                tag.origin().source,
                tag.short(),
                "the wire projection must carry the source name"
            );
        }
    }

    #[test]
    fn clear_prefix_keeps_a_sibling_that_shares_the_prefix_text() {
        let mut map = ProvenanceMap::new();
        map.set("llm.default", SourceTag::Rpc);
        map.set("llm.default_model", SourceTag::Cli);
        map.set("llm.default.endpoint", SourceTag::Rpc);
        map.clear_prefix("llm.default");
        assert!(map.get("llm.default").is_none());
        assert!(map.get("llm.default.endpoint").is_none());
        assert_eq!(map.get("llm.default_model"), Some(&SourceTag::Cli));
    }
}
