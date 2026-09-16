//! Per-leaf provenance for the config store.
//!
//! [`ProvenanceMap`] records, for each dot-joined leaf path, which source
//! last wrote it. An array is one leaf: arrays replace wholesale, so
//! per-element provenance cannot exist.
//!
//! [`ConfigSource`] is a closed set, and every reading of it is a total function
//! of it: its two renderings, its [`ConfigSource::rank`], its
//! [`ConfigSource::origin`] and the refusal rule behind [`ConfigSource::pin`]. A
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

use crate::lua_source::LuaSource;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where one Lua write came from: the source, and the line it sat on.
///
/// Named after Vim's `last_set_sid` and the `:verbose set` output "Last set
/// from …", which is the surface this feeds.
///
/// It embeds [`LuaSource`] rather than re-spelling `plugin` as a bare
/// `String`. Two gains. A plugin write carries the same value the handler
/// registry, the timer registry and `cru.storage` key on, so the config
/// store and the registration side can no longer disagree about who a plugin
/// is. And a human write now distinguishes the user's own `init.lua` from the
/// shipped `runtime/defaults/init.luau`, which were indistinguishable before:
/// both rendered as a `lua` row naming a file, so a default that ships with
/// the daemon read as a line the user wrote.
///
/// # Why there is no load sequence number
///
/// Neovim's `sctx_T` carries one (`sc_seq`), and the question "why don't we"
/// is worth answering rather than leaving as an absence.
///
/// It is load-bearing there because Vimscript gives each *sourcing* of a file
/// its own `s:` scope, so two sourcings of one file must not share
/// script-local variables, and the sequence number is what tells them apart.
/// It also carries Neovim's one refusal rule, which permits a write only when
/// the script id matches AND the sequence differs (`usercmd.c:941`,
/// `userfunc.c:2857`).
///
/// Neither applies here. We have no name-collision refusal for config — a
/// leaf is last-writer-wins by [`ConfigSource::rank`] — and our per-source
/// state is `cru.storage`, whose requirement is the exact OPPOSITE of
/// Vimscript's: storage keyed on a source MUST survive a reload, or a plugin
/// loses its state every time the operator edits a file. So a sequence number
/// in the key would not merely be unread; anything that did read it for
/// storage would be a bug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastSet {
    /// Who wrote it.
    pub source: LuaSource,
    /// The Lua chunk name, usually the file path.
    pub file: String,
    /// The call-site line; `None` for a returned table.
    pub line: Option<u32>,
}

impl LastSet {
    /// One Lua write, from its source and call site.
    #[must_use]
    pub fn new(source: LuaSource, file: impl Into<String>, line: Option<u32>) -> Self {
        Self {
            source,
            file: file.into(),
            line,
        }
    }

    /// The call site as `file:line`, or just the file for a returned table.
    ///
    /// One renderer, because a returned table has no line and two call sites
    /// spelled the absence differently before.
    #[must_use]
    pub fn at(&self) -> String {
        match self.line {
            Some(line) => format!("{}:{line}", self.file),
            None => self.file.clone(),
        }
    }
}

/// Only so `strum::EnumIter` can build the two variants that carry a
/// [`LastSet`], for the gates that walk every layer.
///
/// `cfg(test)`, and hand-written rather than derived, so that [`LuaSource`]
/// does NOT gain a `Default`. A closed set whose variants name real authors
/// must not have one: a defaulted source is a value that means "nobody
/// decided", which is the class of defect this whole type removes.
#[cfg(test)]
impl Default for LastSet {
    fn default() -> Self {
        Self::new(LuaSource::UserLua, String::new(), None)
    }
}

/// Where a config value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum ConfigSource {
    /// The compiled-in default.
    Default,
    /// A plugin's own `setup()` write, made while `init.lua` evaluates.
    ///
    /// The caller decides this layer, not the phase: a plugin file writes a
    /// default even when the user's `init.lua` is the line that called it.
    ///
    /// Serialised as `plugin`, matching [`ConfigSource::short`]. It spelled
    /// itself `plugin_default` on the wire while `short` said `plugin`, which
    /// is one variant with two vocabularies and two different consumers. The
    /// alias keeps an older daemon's `config.effective` parsing.
    #[serde(rename = "plugin", alias = "plugin_default")]
    PluginDefault {
        /// The source, file and line that wrote. `last_set.source` is always
        /// a [`LuaSource::Plugin`] here — that is what makes this the plugin
        /// layer rather than the human one.
        last_set: LastSet,
    },
    /// The machine-written `settings.json`, which the settings UI saves.
    Settings,
    /// The deprecated `config.toml` seed.
    Toml(std::path::PathBuf),
    /// A `cru.config.set` call (or the returned table) in a Lua file the
    /// human owns.
    Lua {
        /// The source, file and line that wrote. `last_set.source` is
        /// [`LuaSource::UserLua`] for the user's own file and
        /// [`LuaSource::Builtin`] for the shipped defaults.
        last_set: LastSet,
    },
    /// The daemon state overlay (`kilns.json`, `llm.json`) — what the daemon
    /// was told through a registration surface.
    Registered,
    /// A CLI flag override.
    Cli,
    /// A runtime `config.set` RPC merge.
    Rpc {
        /// Which RPC client wrote it, when the seam knew.
        ///
        /// A FIELD, not a variant, after `sctx_T`'s `sc_chan`: the layer is
        /// one layer whatever client reaches it, and a variant per client is
        /// not a closed set. Without it every client flattens into one `rpc`
        /// row, so the settings pane cannot say a value came from somewhere
        /// else this run — and with the pin being config's only enforcement,
        /// saying so IS the answer.
        ///
        /// `None` where no client is in scope: a `cru.config.set` from an
        /// eval, and every in-process merge.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chan: Option<u64>,
    },
}

/// Where one leaf came from, in the shape the wire uses.
///
/// One projection serves two callers: `config.origin` answers with it, and a
/// `config.save` refusal names the pin with it. Two projections would let the
/// refusal name a file the origin does not.
/// `Deserialize` as well as `Serialize`: `config.save`'s refusal travels back
/// over the RPC into a typed reply, and a `&'static str` cannot be read from a
/// document. The value is still [`ConfigSource::short`]'s, written once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceOrigin {
    /// The one-word source name, as [`ConfigSource::short`] gives it.
    pub source: String,
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

impl ConfigSource {
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
            ConfigSource::Default => 0,
            ConfigSource::PluginDefault { .. } => 1,
            ConfigSource::Settings => 2,
            ConfigSource::Toml(_) => 3,
            ConfigSource::Lua { .. } => 4,
            ConfigSource::Registered => 5,
            ConfigSource::Cli => 6,
            ConfigSource::Rpc { .. } => 7,
        }
    }

    /// Whether a `config.save` under this source would act nowhere.
    ///
    /// This is the whole refusal rule of `config.save`, and it is one rule for
    /// one reason: a save writes the `Settings` layer, so it acts only on a
    /// leaf that no *undroppable* layer above `Settings` holds. A leaf that
    /// fails that test is refused rather than accepted — the store used to
    /// accept it, hand it to the caller for `settings.json`, and then let the
    /// rank gate throw it away, which is an `ok: true` for a write that
    /// changed nothing.
    ///
    /// "Undroppable" is what separates the two layers that outrank `Settings`
    /// and pin nothing from the two that outrank it and do. `Rpc` is dropped
    /// by the save itself ([`ConfigSource::reset_drops`]), so it cannot block
    /// one; `Cli` is not, so it can.
    fn pins_a_leaf(&self) -> bool {
        match self {
            // Both load below `Settings`, so a save wins over them and acts.
            // The plugin layer is below on purpose: plugins run while
            // `init.lua` evaluates, and if their writes pinned, nearly every
            // key would lock and the settings UI would be useless.
            ConfigSource::Default | ConfigSource::PluginDefault { .. } => false,
            // The layer `config.save` writes. It cannot pin against itself.
            ConfigSource::Settings => false,
            // Two files a person owns. Both re-apply at every boot.
            ConfigSource::Toml(_) | ConfigSource::Lua { .. } => true,
            // Daemon state (`kilns.json`, `llm.json`). It reloads at the next
            // boot, and a `settings.json` entry that half-described the same
            // provider would shadow the working one, so a save under it is
            // worse than lost. It names no file a person edits; `cru kiln
            // register` and the provider surfaces are the route to change it.
            //
            // THIS ARM IS WHY THE PIN CANNOT BE REPLACED BY A RANK
            // COMPARISON. Every other pinning layer has a layer in the store,
            // so a save could in principle be refused by comparing ranks. The
            // state overlay does NOT: its leaves are not in the store at all,
            // which is why `config.save` takes an `also_pinned` closure and
            // why `fold_state_overlay` in the daemon's `rpc/dispatch.rs` is
            // its only production supplier. Drop the pin and a
            // `cru models embeddings use <NAME>` reports the change, tells the
            // user to restart, and comes back on the old model.
            ConfigSource::Registered => true,
            // A flag typed for this invocation. It dies with the process, but
            // it outranks `Settings` for the whole of this run and no save
            // drops it — so a value saved under it would not move the screen.
            // Refusing names the flag; accepting wrote a file and did nothing.
            ConfigSource::Cli => true,
            // The runtime knob `:set` writes. It outranks `Settings` too, but
            // the save drops the ephemeral hold on every leaf it accepts, so
            // it is gone by the time the merge reads the rank.
            ConfigSource::Rpc { .. } => false,
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
            ConfigSource::Default | ConfigSource::PluginDefault { .. } => false,
            // The durable machine layer. See the doc comment above.
            ConfigSource::Settings => false,
            // Two files a person owns. Both re-apply at every boot.
            ConfigSource::Toml(_) | ConfigSource::Lua { .. } => false,
            // Daemon state (`kilns.json`, `llm.json`). It reloads at the next
            // boot, and `cru kiln register` is the route to change it.
            ConfigSource::Registered => false,
            // A flag the operator typed for this invocation. It dies with the
            // process, but it is the terms the daemon was started on, and a
            // keystroke in one client must not erase them for every client.
            ConfigSource::Cli => false,
            // The ephemeral runtime knob `:set` writes: the one layer a
            // running session authors, and the one `&` undoes.
            ConfigSource::Rpc { .. } => true,
        }
    }

    /// Where the leaf came from, for `config.origin`.
    pub fn origin(&self) -> SourceOrigin {
        let (file, line) = match self {
            ConfigSource::PluginDefault { last_set } | ConfigSource::Lua { last_set } => {
                (Some(last_set.file.clone()), last_set.line)
            }
            ConfigSource::Toml(path) => (Some(path.display().to_string()), None),
            ConfigSource::Default
            | ConfigSource::Settings
            | ConfigSource::Registered
            | ConfigSource::Cli
            | ConfigSource::Rpc { .. } => (None, None),
        };
        SourceOrigin {
            source: self.short().to_string(),
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
            ConfigSource::Default => "default",
            ConfigSource::PluginDefault { .. } => "plugin",
            ConfigSource::Settings => "settings",
            ConfigSource::Toml(_) => "toml",
            ConfigSource::Lua { .. } => "lua",
            ConfigSource::Registered => "registered",
            ConfigSource::Cli => "cli",
            ConfigSource::Rpc { .. } => "rpc",
        }
    }

    /// The rendered detail, for `cru config show --sources`.
    pub fn detail(&self) -> String {
        match self {
            ConfigSource::Default => "default".to_string(),
            // The source NAMES itself here — a plugin renders its own name,
            // and the two human sources render `init.lua` or `builtin`. That
            // is what embedding `LuaSource` buys: the shipped defaults used
            // to render as an anonymous `lua` row.
            ConfigSource::PluginDefault { last_set } => {
                format!("plugin {} ({})", last_set.source, last_set.at())
            }
            ConfigSource::Settings => "settings".to_string(),
            ConfigSource::Toml(path) => format!("toml ({})", path.display()),
            ConfigSource::Lua { last_set } => format!("lua ({})", last_set.at()),
            ConfigSource::Registered => "registered".to_string(),
            ConfigSource::Cli => "cli".to_string(),
            // A channel id renders when the seam knew one, so two clients
            // reading `--sources` can tell their own write from the other's.
            ConfigSource::Rpc { chan: Some(chan) } => format!("rpc (channel {chan})"),
            ConfigSource::Rpc { chan: None } => "rpc".to_string(),
        }
    }
}

/// Dot-joined leaf path → the source that last wrote it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvenanceMap {
    entries: BTreeMap<String, ConfigSource>,
}

impl ProvenanceMap {
    /// An empty map. Every absent entry renders as `default`.
    pub fn new() -> Self {
        Self::default()
    }

    /// The source recorded for `path`, if any.
    pub fn get(&self, path: &str) -> Option<&ConfigSource> {
        self.entries.get(path)
    }

    /// Record `tag` for one leaf path.
    pub fn set(&mut self, path: impl Into<String>, tag: ConfigSource) {
        self.entries.insert(path.into(), tag);
    }

    /// Drop every entry at `prefix` or under it.
    ///
    /// A leaf write clears first, then records its own row. A terminal value
    /// written where a table stood takes that table's leaves out of the value,
    /// so their rows would otherwise linger as ghosts.
    pub fn clear_prefix(&mut self, prefix: &str) {
        let child_prefix = format!("{prefix}.");
        self.entries
            .retain(|path, _| path != prefix && !path.starts_with(&child_prefix));
    }

    /// Iterate the recorded entries in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ConfigSource)> {
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
        let mut sorted: Vec<ConfigSource> = ConfigSource::iter().collect();
        sorted.sort_by_key(ConfigSource::rank);
        let order: Vec<&str> = sorted.iter().map(ConfigSource::short).collect();
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
        let mut ranks: Vec<u8> = ConfigSource::iter().map(|tag| tag.rank()).collect();
        let count = ranks.len();
        ranks.sort_unstable();
        ranks.dedup();
        assert_eq!(ranks.len(), count, "two ConfigSource variants share a rank");
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
        let mut names: Vec<&str> = ConfigSource::iter().map(|tag| tag.short()).collect();
        let count = names.len();
        assert!(names.iter().all(|name| !name.is_empty()));
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            count,
            "two ConfigSource variants share a short name"
        );
    }

    /// A variant's serde tag and its [`ConfigSource::short`] name must be the
    /// SAME word.
    ///
    /// Two vocabularies for one variant is what this gate exists to stop, and
    /// it had already happened: `PluginDefault` serialised as `plugin_default`
    /// while `short` said `plugin`. Both are observable and they have
    /// different consumers — `short` reaches the web settings pane through
    /// `SourceOrigin.source`, and the serde tag reaches the CLI through the
    /// `provenance` field of `config.effective` — so nothing compared them and
    /// they drifted.
    ///
    /// Both sides are DERIVED here, over `EnumIter`, rather than a hardcoded
    /// table of pairs: a table would need updating by the same person who
    /// introduced the third spelling.
    #[test]
    fn every_variant_serialises_as_the_word_short_names_it() {
        for tag in ConfigSource::iter() {
            let json = serde_json::to_value(&tag).expect("a layer serialises");
            // A unit variant serialises as a bare string; one with fields
            // serialises as a single-key object. Either way the tag is the
            // word, and that word must be `short`.
            // Read the tag without matching on `serde_json::Value` — this
            // module denies a wildcard arm, and an external enum cannot be
            // matched exhaustively here.
            let single_key = json
                .as_object()
                .filter(|map| map.len() == 1)
                .and_then(|map| map.keys().next().cloned());
            let serde_tag = json
                .as_str()
                .map(str::to_string)
                .or(single_key)
                .unwrap_or_else(|| {
                    panic!(
                        "'{}' serialises as neither a string nor a single-key object: {json}",
                        tag.short()
                    )
                });
            assert_eq!(
                serde_tag,
                tag.short(),
                "'{}' carries two vocabularies: serde says '{serde_tag}', short says '{}'",
                tag.short(),
                tag.short()
            );
        }
    }

    /// The wire spelling `plugin_default` still parses, so a new client
    /// reading an older daemon's `config.effective` does not lose its
    /// `--sources` detail.
    ///
    /// The alias is the only reason the rename above is safe to make, so it
    /// needs a gate of its own — an alias nothing exercises is an alias
    /// somebody deletes.
    #[test]
    fn the_old_plugin_default_wire_spelling_still_parses() {
        let wire = serde_json::json!({
            "plugin_default": {
                "last_set": {
                    "source": { "plugin": "alpha" },
                    "file": "/plugins/alpha/init.lua",
                    "line": 4
                }
            }
        });
        let parsed: ConfigSource =
            serde_json::from_value(wire).expect("the superseded spelling must still parse");
        assert_eq!(parsed.short(), "plugin");
        assert_eq!(
            parsed.origin().file.as_deref(),
            Some("/plugins/alpha/init.lua")
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
        let droppable: Vec<ConfigSource> = ConfigSource::iter()
            .filter(ConfigSource::reset_drops)
            .collect();
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

    /// Two steps of the layer order, stated on the enum alone: a plugin's
    /// `setup()` write ranks below `Settings`, and a human's own line ranks
    /// above it.
    ///
    /// This test reads `rank` and nothing else, so it cannot see whether the
    /// merge still consults `rank`. The VALUE gates for the same two steps
    /// live in `store.rs` — `a_plugin_default_never_overwrites_what_the_user_saved`
    /// and `a_human_lua_line_takes_a_leaf_the_settings_layer_holds`. Both go
    /// red when the rank gate in `ConfigStore::apply` goes; this one stays
    /// green, which is why it promises the order and not the outcome.
    #[test]
    fn the_plugin_layer_ranks_below_settings_and_a_human_line_ranks_above() {
        let plugin = ConfigSource::PluginDefault {
            last_set: LastSet::new(
                LuaSource::Plugin("alpha".to_string()),
                "/plugins/alpha/init.lua".to_string(),
                Some(4),
            ),
        };
        let human = ConfigSource::Lua {
            last_set: LastSet::new(LuaSource::UserLua, "/config/init.lua".to_string(), Some(4)),
        };
        assert!(plugin.rank() < ConfigSource::Settings.rank());
        assert!(human.rank() > ConfigSource::Settings.rank());
    }

    /// The two layers a `config.save` must be able to write over. A plugin's
    /// declared default is the load-bearing one: if it pinned, nearly every
    /// key would lock and the settings UI would refuse almost every save.
    #[test]
    fn a_plugin_default_and_the_settings_layer_pin_nothing() {
        let plugin = ConfigSource::PluginDefault {
            last_set: LastSet::new(
                LuaSource::Plugin("alpha".to_string()),
                "/plugins/alpha/init.lua".to_string(),
                Some(4),
            ),
        };
        assert_eq!(plugin.pin(), None);
        assert_eq!(ConfigSource::Settings.pin(), None);
        assert_eq!(ConfigSource::Default.pin(), None);
    }

    /// `:set` is the ephemeral knob. It must not pin, or one `:set` would
    /// lock the settings UI out of that key for the rest of the process.
    #[test]
    fn the_runtime_knob_pins_nothing() {
        assert_eq!(ConfigSource::Rpc { chan: None }.pin(), None);
    }

    /// The refusal rule, derived rather than restated: a save writes the
    /// `Settings` layer, so exactly the layers that outrank it AND survive the
    /// drop the save performs may refuse one.
    ///
    /// This is a cross-check on the enumerated table, not the gate. The gates
    /// assert the VALUES a store answers with — see
    /// `a_save_the_rank_gate_would_drop_is_refused_and_carries_nothing_to_the_file`
    /// and `every_layer_that_refuses_a_save_beats_the_settings_layer_in_the_store`
    /// in `store.rs`, the second of which walks these same variants through a
    /// store. A layer that pins when it should not locks the settings UI out
    /// of a key; one that does not pin when it should lets a save write a file
    /// that changes nothing.
    #[test]
    fn exactly_the_layers_a_save_cannot_overwrite_refuse_one() {
        for tag in ConfigSource::iter() {
            let survives_the_save =
                tag.rank() > ConfigSource::Settings.rank() && !tag.reset_drops();
            assert_eq!(
                tag.pin().is_some(),
                survives_the_save,
                "'{}' ranks {} and reset_drops={}, so a save {} it",
                tag.short(),
                tag.rank(),
                tag.reset_drops(),
                if survives_the_save {
                    "cannot overwrite"
                } else {
                    "overwrites"
                }
            );
        }
    }

    /// A refused save names the line to change, so the pin carries the file
    /// and the line the human wrote.
    #[test]
    fn a_human_lua_line_pins_with_its_file_and_line() {
        let pin = ConfigSource::Lua {
            last_set: LastSet::new(
                LuaSource::UserLua,
                "/home/u/.config/crucible/init.lua".to_string(),
                Some(12),
            ),
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
        for tag in ConfigSource::iter() {
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
        map.set("llm.default", ConfigSource::Rpc { chan: None });
        map.set("llm.default_model", ConfigSource::Cli);
        map.set("llm.default.endpoint", ConfigSource::Rpc { chan: None });
        map.clear_prefix("llm.default");
        assert!(map.get("llm.default").is_none());
        assert!(map.get("llm.default.endpoint").is_none());
        assert_eq!(map.get("llm.default_model"), Some(&ConfigSource::Cli));
    }
}
