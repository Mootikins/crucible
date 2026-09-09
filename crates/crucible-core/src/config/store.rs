//! The one config store: a JSON object, its provenance, and the phase flag.
//!
//! One store serves two phases instead of two stores in two VMs (§9 of the
//! execution plan). During the boot evaluation of `init.lua` the policy is
//! [`LocationPolicy::Accept`]: location keys are legitimate config
//! authorship. After the daemon extracts the config and freezes the location
//! slice, the policy is [`LocationPolicy::Withhold`]: location keys are
//! withheld from every merge and reported, because the RPC socket has no
//! authentication and these keys answer *where the daemon acts*.

use serde::Serialize;
use serde_json::Value;

use super::config::{CliAppConfig, ConfigError, LOCATION_CONFIG_KEYS};
use super::merge::{flatten_leaves, nest_leaves, set_leaf};
use super::provenance::{LeafOrigin, ProvenanceMap, SourceOrigin, SourceTag};

/// One leaf `config.save` refuses, and what pins it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PinnedLeaf {
    /// The dot-joined leaf path the caller asked to save.
    pub key: String,
    /// The source that holds the leaf, with the file and line when it has
    /// them, so the caller can offer a jump to that line.
    #[serde(flatten)]
    pub pin: SourceOrigin,
}

/// What one [`ConfigStore::save`] did with an overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSettings {
    /// The part of the overlay that reached the `Settings` layer. The caller
    /// writes exactly this to `settings.json`, so the file and the live store
    /// hold one delta.
    pub accepted: Value,
    /// The leaves a pin refused, each with the file and line that hold it.
    pub refused: Vec<PinnedLeaf>,
    /// The top-level location keys the policy withheld from the merge.
    pub withheld: Vec<String>,
}

/// Whether a merge may write the keys that name a filesystem location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationPolicy {
    /// Load time: location keys are legitimate config authorship.
    Accept,
    /// Daemon runtime: location keys are withheld and reported.
    Withhold,
}

/// What [`ConfigStore::reset`], [`ConfigStore::pop`] or [`ConfigStore::unset`]
/// did to one leaf.
///
/// Not `Serialize`: the RPC answers with this *and* with the leaf's new
/// origin row, and one hand-built object is what keeps the two halves of that
/// answer in one shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerDrop {
    /// The leaf's top-level key names where the daemon acts, and the runtime
    /// policy withholds it from every write door. The same rule
    /// [`ConfigStore::merge`] applies to an overlay: a caller that could pop
    /// `runtimepath` would re-point the trees the daemon reads code from
    /// without the floor ever seeing a path.
    Withheld,
    /// Nothing was dropped. No retained layer of the kind asked for held the
    /// leaf, so the value stands.
    Untouched,
    /// The sources whose hold on the leaf was dropped, lowest rank first.
    Dropped(Vec<SourceTag>),
}

/// One merge, retained so the store can be rebuilt without part of it.
///
/// The WHOLE overlay is kept, including the leaves the rank gate refused. A
/// refused write is exactly the layer a later [`ConfigStore::pop`] must
/// reveal — a plugin default that lost the leaf to `settings.json` is what
/// sits under `settings.json`.
#[derive(Debug, Clone)]
struct Layer {
    source: SourceTag,
    /// The overlay as leaves: one dot-joined path per terminal value, in the
    /// order the caller wrote them.
    ///
    /// Flattened once, at the door, rather than at every replay. A drop verb
    /// then removes a key rather than walking a tree, and the paths a replay
    /// offers the rank gate are the paths the first merge offered it.
    leaves: serde_json::Map<String, Value>,
    /// The policy in force when this layer was FIRST applied.
    ///
    /// The layer replays under its own policy, not the store's current one.
    /// The two questions differ: "may this WRITE name a location" is asked
    /// once, at the door, and a boot layer legitimately answered yes; "does
    /// the plugin-visible value hold a location" is asked of the whole store,
    /// and [`ConfigStore::rebuild`] answers it by stripping the value at the
    /// end. A replay under the CURRENT policy conflated them: it withheld the
    /// boot layer's location keys, so the provenance and the pins the replay
    /// rebuilds from nothing lost every row a boot layer had left on them —
    /// and `config.effective` reads exactly those rows to tell a client
    /// whether anybody configured `kiln_path`.
    policy: LocationPolicy,
}

/// A merged config value with per-leaf provenance.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    /// Always a JSON object.
    value: Value,
    provenance: ProvenanceMap,
    /// The leaves a boot-restored layer holds, and which layer holds them.
    ///
    /// A second map, because `provenance` answers a different question. It
    /// names whoever wrote last, and the ephemeral `:set` writes last all the
    /// time. The pin is what re-applies at the NEXT boot, so an ephemeral
    /// write must not erase it — otherwise one `:set` would open a pinned key
    /// to `config.save` and the saved value would vanish at the next boot.
    pins: ProvenanceMap,
    location_policy: LocationPolicy,
    /// Every merge, in the order it was made.
    ///
    /// **The store re-merges rather than keeping an undo stack per leaf.**
    /// `pop` has to answer "what would the merge have given without this
    /// write", and the only answer that cannot drift from the merge is the
    /// merge: drop the leaf from the layer that holds it, then replay the
    /// layers. A per-leaf stack of superseded values would be a second copy
    /// of the layer order — `SourceTag::rank` and the pin rule all over again
    /// — and the two copies would disagree the first time one of them changed.
    ///
    /// The list grows by one entry per merge and nothing prunes it. Every
    /// writer is a boot step, a `cru` command or a keystroke, so the rate is
    /// a handful of small objects per session.
    layers: Vec<Layer>,
}

impl ConfigStore {
    /// A boot-phase store: location keys are accepted.
    pub fn for_load() -> Self {
        Self {
            value: Value::Object(serde_json::Map::new()),
            provenance: ProvenanceMap::new(),
            pins: ProvenanceMap::new(),
            location_policy: LocationPolicy::Withhold,
            layers: Vec::new(),
        }
        .with_policy(LocationPolicy::Accept)
    }

    /// A runtime store: location keys are withheld and reported.
    pub fn runtime() -> Self {
        Self {
            value: Value::Object(serde_json::Map::new()),
            provenance: ProvenanceMap::new(),
            pins: ProvenanceMap::new(),
            location_policy: LocationPolicy::Withhold,
            layers: Vec::new(),
        }
    }

    fn with_policy(mut self, policy: LocationPolicy) -> Self {
        self.location_policy = policy;
        self
    }

    /// The phase in force.
    pub fn location_policy(&self) -> LocationPolicy {
        self.location_policy
    }

    /// Merge `overlay` into the store, recording provenance, and return the
    /// top-level location keys the policy withheld (empty under
    /// [`LocationPolicy::Accept`]).
    ///
    /// **One write, one leaf.** The overlay is flattened at the door
    /// ([`flatten_leaves`]), so a nested table is authoring sugar for a set of
    /// writes at dot-joined paths and the store records exactly one row per
    /// terminal value. A nested write therefore keeps the siblings it does not
    /// name — it never names them — and it cannot remove a key: that verb is
    /// [`Self::unset`].
    ///
    /// The merge is per-leaf and ranked: a leaf a higher layer already holds
    /// keeps its value and its provenance, and the write lands on every other
    /// leaf of the same overlay. See [`SourceTag::rank`] for the order.
    ///
    /// A non-object overlay is refused wholesale: the store's value is always
    /// an object, and no caller has a scalar to contribute at the root.
    pub fn merge(&mut self, overlay: Value, source: SourceTag) -> Vec<String> {
        let leaves: serde_json::Map<String, Value> = flatten_leaves(overlay).into_iter().collect();
        // Retain the layer before it lands, so `reset`, `pop` and `unset` can
        // replay the store without part of it.
        self.layers.push(Layer {
            source: source.clone(),
            leaves: leaves.clone(),
            policy: self.location_policy,
        });
        self.apply(&leaves, source, self.location_policy)
    }

    /// Land one layer's leaves on the value, the provenance and the pins, and
    /// answer with the location keys the policy withheld.
    ///
    /// The half of [`Self::merge`] a replay repeats. `policy` is the one that
    /// governed the WRITE — the store's own for a merge, the layer's own for
    /// a replay — so a boot layer keeps the location keys it legitimately
    /// authored and a runtime write is still refused whole. Keeping a location
    /// out of the plugin-visible value is the other half, and [`Self::rebuild`]
    /// does it once at the end.
    ///
    /// It records no layer of its own — [`Self::rebuild`] walks the layers it
    /// already has, and a second push would double them on every drop.
    fn apply(
        &mut self,
        leaves: &serde_json::Map<String, Value>,
        source: SourceTag,
        policy: LocationPolicy,
    ) -> Vec<String> {
        let mut withheld: Vec<String> = Vec::new();
        // Only a pinning source may write or clear a pin. An ephemeral `:set`
        // replaces the value and the provenance row, but the line that wrote
        // the leaf still runs at the next boot, so the pin outlives it.
        let pinning = source.pin().is_some();
        let rank = source.rank();
        for (path, value) in leaves {
            if policy == LocationPolicy::Withhold {
                // The TOP-LEVEL key, because that is the granularity
                // `LOCATION_CONFIG_KEYS` classifies: `kilns.notes` is part of
                // `kilns`, and one report names `kilns` once.
                let head = path.split('.').next().unwrap_or(path);
                if LOCATION_CONFIG_KEYS.contains(&head) {
                    if !withheld.iter().any(|seen| seen == head) {
                        withheld.push(head.to_string());
                    }
                    continue;
                }
            }
            // Layer precedence, decided per leaf by `SourceTag::rank` rather
            // than by the order the layers happen to merge in. The order
            // inverts the rule: `settings.json` merges before `init.lua` is
            // evaluated, and a plugin writes DURING that evaluation — so the
            // plugin always writes last, and last-write-wins handed it the
            // leaf the user saved. Equal rank still writes: two lines of one
            // file are ordered by the file, and the second is the one the
            // author meant.
            //
            // ONE row at ONE path, because the write is one leaf. The store
            // used to ask for the highest rank in the whole subtree, which is
            // the question a wholesale write raised and a flat write does not.
            if self
                .provenance
                .get(path)
                .is_some_and(|held| rank < held.rank())
            {
                continue;
            }
            set_leaf(&mut self.value, path, value.clone());
            // A terminal value written where a table stood takes the table's
            // leaves out of the value, so their rows go too.
            self.provenance.clear_prefix(path);
            self.provenance.set(path, source.clone());
            if pinning {
                self.pins.clear_prefix(path);
                self.pins.set(path, source.clone());
            }
        }
        withheld
    }

    /// Drop the runtime knob's hold on one leaf: `config.reset`, which the
    /// TUI spells `:set key&`.
    ///
    /// It drops every retained layer that [`SourceTag::reset_drops`] names —
    /// the ephemeral `config.set` writes, and nothing else — then replays.
    /// The leaf therefore returns to what the compiled defaults, the plugin
    /// defaults and the config files give it, which is the value the next
    /// boot would give it too.
    pub fn reset(&mut self, path: &str) -> LayerDrop {
        if self.withholds(path) {
            return LayerDrop::Withheld;
        }
        self.drop_where(path, LeafScope::Exact, |_, source| source.reset_drops())
    }

    /// Remove a key and everything under it from the layers a reset may drop:
    /// `config.unset`.
    ///
    /// **The one thing a flat write cannot say.** `config.set` writes one leaf
    /// per terminal value, so it can add `llm.providers.mine.endpoint` and it
    /// can change it, but it can never say "this provider is gone" — which is
    /// the one need the deleted `__replace` marker really served. `unset` is
    /// that verb, and it is a PREFIX drop where [`Self::reset`] is a leaf drop.
    ///
    /// It reaches exactly the layers [`SourceTag::reset_drops`] names, for the
    /// same reason `reset` does: every other layer is restored by a file or by
    /// the invocation, and removing one in memory would answer with a store
    /// the next boot takes straight back. **It edits no file.** To drop a
    /// provider `init.lua` declares, edit `init.lua`; to drop one
    /// `settings.json` holds, save over it.
    pub fn unset(&mut self, prefix: &str) -> LayerDrop {
        self.drop_where(prefix, LeafScope::Subtree, |_, source| source.reset_drops())
    }

    /// Drop the highest-ranked layer that holds one leaf, revealing the next
    /// one down: `config.pop`, which the TUI spells `:set key^`.
    ///
    /// Highest RANK, not latest write, because rank is what decides the leaf:
    /// a plugin default written after `settings.json` never held the leaf, so
    /// dropping it would reveal nothing. Ties go to the last of them, which
    /// is the write the merge kept.
    ///
    /// The drop is in memory only. Every layer returns at the next boot, and
    /// no verb here edits a file the user owns.
    pub fn pop(&mut self, path: &str) -> LayerDrop {
        if self.withholds(path) {
            return LayerDrop::Withheld;
        }
        let top = self
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.leaves.contains_key(path))
            .max_by_key(|(index, layer)| (layer.source.rank(), *index))
            .map(|(index, _)| index);
        let Some(top) = top else {
            return LayerDrop::Untouched;
        };
        self.drop_where(path, LeafScope::Exact, |index, _| index == top)
    }

    /// Take `path` (or the subtree under it) out of every layer `wanted`
    /// names, and replay what is left.
    ///
    /// One implementation for the three drop verbs, because they differ only
    /// in which layers they reach and how much of the path they take. The
    /// replay is shared, so none of them can invent a merge rule of its own.
    fn drop_where(
        &mut self,
        path: &str,
        scope: LeafScope,
        wanted: impl Fn(usize, &SourceTag) -> bool,
    ) -> LayerDrop {
        let mut dropped = Vec::new();
        for (index, layer) in self.layers.iter_mut().enumerate() {
            if !wanted(index, &layer.source) {
                continue;
            }
            if remove_leaves(&mut layer.leaves, path, scope) {
                dropped.push(layer.source.clone());
            }
        }
        if dropped.is_empty() {
            return LayerDrop::Untouched;
        }
        dropped.sort_by_key(SourceTag::rank);
        self.rebuild();
        LayerDrop::Dropped(dropped)
    }

    /// Whether the policy withholds the top-level key `path` names.
    ///
    /// The top-level key, because that is the granularity
    /// [`LOCATION_CONFIG_KEYS`] classifies: `kilns.notes` is part of `kilns`.
    fn withholds(&self, path: &str) -> bool {
        if self.location_policy == LocationPolicy::Accept {
            return false;
        }
        let head = path.split('.').next().unwrap_or(path);
        LOCATION_CONFIG_KEYS.contains(&head) || LOCATION_CONFIG_KEYS.contains(&path)
    }

    /// Merge the retained layers again, from nothing.
    ///
    /// The replay is the point: the value, the provenance and the pins a drop
    /// leaves behind are whatever [`Self::merge`] gives for the layers that
    /// remain, so the drop cannot invent a rule of its own.
    ///
    /// Each layer replays under the policy it was first applied with, and the
    /// strip at the end reproduces [`Self::end_boot_phase`]'s post-condition:
    /// the boot layers keep their rows on the location keys, and the
    /// plugin-visible value holds none of them.
    fn rebuild(&mut self) {
        let layers = std::mem::take(&mut self.layers);
        self.value = Value::Object(serde_json::Map::new());
        self.provenance = ProvenanceMap::new();
        self.pins = ProvenanceMap::new();
        for layer in &layers {
            let _ = self.apply(&layer.leaves, layer.source.clone(), layer.policy);
        }
        self.layers = layers;
        if self.location_policy == LocationPolicy::Withhold {
            self.strip_locations_from_value();
        }
    }

    /// End the boot phase: withhold location keys from every later merge, and
    /// drop them from the stored value.
    ///
    /// The daemon extracts the full config (locations included) *before*
    /// calling this; what remains is the plugin-visible view, which never
    /// holds a location — a plugin knows kilns by name, and handing it the
    /// directories through `cru.config.get` would make that pointless.
    pub fn end_boot_phase(&mut self) {
        self.location_policy = LocationPolicy::Withhold;
        self.strip_locations_from_value();
    }

    /// Take every location key out of the merged value, and leave the
    /// provenance and the pins alone.
    ///
    /// The TOP-LEVEL key, because that is the granularity
    /// [`LOCATION_CONFIG_KEYS`] classifies: removing `kilns` removes every
    /// leaf under it. The rows stay on purpose — they answer "did anybody
    /// configure this", which is a different question from "may a plugin read
    /// it", and `config.effective` needs the first one to keep a client from
    /// substituting its own working directory.
    fn strip_locations_from_value(&mut self) {
        if let Value::Object(map) = &mut self.value {
            for key in LOCATION_CONFIG_KEYS {
                map.remove(key);
            }
        }
    }

    /// The pin on one dot-joined leaf path, if a boot-restored layer above
    /// `Settings` holds it.
    pub fn pin(&self, path: &str) -> Option<SourceOrigin> {
        self.pins.get(path).and_then(SourceTag::pin)
    }

    /// Split `overlay` into the part `config.save` may write and the leaves a
    /// pin refuses.
    ///
    /// One flatten answers both halves, so they cannot disagree about a leaf.
    /// It is the same [`flatten_leaves`] the merge uses, which is the point:
    /// the split used to have a leaf rule of its own, and a branch it called a
    /// leaf was a branch the merge walked into.
    pub fn split_pinned(&self, overlay: Value) -> (Value, Vec<PinnedLeaf>) {
        split_pinned_by(overlay, &|path| self.pin(path))
    }

    /// Save `overlay` as the durable `Settings` layer, and give the saved
    /// leaves back to it: the ephemeral hold on each accepted leaf goes
    /// first.
    ///
    /// The drop is what makes the save act. `Rpc` outranks `Settings`, so a
    /// leaf a `:set` already holds keeps the scratch value: the settings UI
    /// wrote the file, the screen did not move, and the saved value appeared
    /// only after a restart. That is a write the system accepts and does not
    /// apply. The drop is the one [`Self::reset`] performs, and it reaches
    /// only the leaves of this overlay — another client's `:set` on an
    /// unrelated key stands.
    ///
    /// **The refusal and the drop read one flatten, so they cannot disagree.**
    /// A leaf a pin refuses is neither merged nor dropped, which is what keeps
    /// a refused save from undoing the `:set` the user raised over a pinned
    /// line. Two walks would answer "which leaves am I saving" twice, and the
    /// second answer would clear a value nothing replaced — which is what a
    /// split with a leaf rule of its own did: it called a BRANCH a leaf, and
    /// removed the whole subtree under it from the ephemeral layer.
    ///
    /// A leaf the merge would then drop is refused rather than accepted (see
    /// [`SourceTag::pin`]), so `accepted` never carries a value that would not
    /// act — the caller writes exactly it to `settings.json`.
    ///
    /// `also_pinned` carries the pins the store cannot see: the daemon's
    /// state overlay (`llm.json`) holds leaves no layer here carries.
    pub fn save(
        &mut self,
        overlay: Value,
        also_pinned: &dyn Fn(&str) -> Option<SourceOrigin>,
    ) -> SavedSettings {
        let mut refused = Vec::new();
        let mut kept: Vec<(String, Value)> = Vec::new();
        for (path, value) in flatten_leaves(overlay) {
            match self.pin(&path).or_else(|| also_pinned(&path)) {
                Some(pin) => refused.push(PinnedLeaf { key: path, pin }),
                None => kept.push((path, value)),
            }
        }
        let mut dropped = false;
        for layer in self
            .layers
            .iter_mut()
            .filter(|layer| layer.source.reset_drops())
        {
            for (path, _) in &kept {
                dropped |= layer.leaves.shift_remove(path).is_some();
            }
        }
        if dropped {
            // Before the merge, not after: the rank gate reads the provenance
            // the store holds now, and a stale `Rpc` row would refuse the very
            // write this drop was made for.
            self.rebuild();
        }
        // Nested again for the caller: `settings.json` stays nested JSON, so a
        // person can read it and a UI can round-trip it. The merge flattens it
        // straight back, and the round trip is the identity.
        let accepted = nest_leaves(kept);
        let withheld = self.merge(accepted.clone(), SourceTag::Settings);
        SavedSettings {
            accepted,
            refused,
            withheld,
        }
    }

    /// The merged value. Always an object.
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Per-leaf provenance for the merged value.
    pub fn provenance(&self) -> &ProvenanceMap {
        &self.provenance
    }

    /// Where one leaf came from, and whether [`Self::split_pinned`] refuses
    /// it.
    ///
    /// **A pin outranks the last writer here, because the two answer
    /// different questions.** `provenance` names whoever wrote last, and the
    /// ephemeral `:set` writes last all the time. A settings UI asks a
    /// different thing: may I save this leaf, and which line stops me. If
    /// this row read `provenance`, one routine `:set` would report the leaf
    /// as unpinned and file-less, the control would unlock, and the save it
    /// invited would come back refused by a file the row never named.
    ///
    /// One projection therefore serves both callers, and the refusal cannot
    /// name a file the origin does not.
    ///
    /// A leaf no map recorded defaulted: `merge` records a provenance row for
    /// every leaf it writes, so an absent row means nothing ever wrote it.
    pub fn origin(&self, path: &str) -> LeafOrigin {
        match self.pin(path) {
            Some(pin) => LeafOrigin {
                pinned: true,
                origin: pin,
            },
            None => LeafOrigin {
                pinned: false,
                origin: self
                    .provenance
                    .get(path)
                    .unwrap_or(&SourceTag::Default)
                    .origin(),
            },
        }
    }

    /// Every leaf either map recorded, in path order.
    ///
    /// The union, not the provenance map alone. A terminal value written where
    /// a table stood clears the provenance rows under it, while the pin
    /// survives — so a leaf can still be refused by a line after its
    /// provenance row is gone. A listing built from `provenance` would drop
    /// exactly the leaves a UI must render locked.
    pub fn recorded_leaves(&self) -> Vec<&str> {
        let mut leaves: Vec<&str> = self
            .provenance
            .iter()
            .chain(self.pins.iter())
            .map(|(path, _)| path.as_str())
            .collect();
        leaves.sort_unstable();
        leaves.dedup();
        leaves
    }

    /// Extract the typed config from the merged value.
    ///
    /// Unknown keys are ignored by extraction and stay in the store — plugins
    /// own free-form `plugins.<name>` keys.
    pub fn extract(&self) -> Result<CliAppConfig, ConfigError> {
        serde_json::from_value(self.value.clone()).map_err(ConfigError::Serialization)
    }
}

/// Split `overlay` into the part a caller may write and the leaves `pin`
/// refuses.
///
/// Free rather than a method, because a pin is not always the store's own:
/// the daemon's state overlay (`llm.json`) holds leaves no layer of the store
/// carries. [`ConfigStore::save`] weighs both hands and flattens the same way,
/// so the refusal cannot mean one thing here and another there.
pub fn split_pinned_by(
    overlay: Value,
    pin: &dyn Fn(&str) -> Option<SourceOrigin>,
) -> (Value, Vec<PinnedLeaf>) {
    let mut refused = Vec::new();
    let mut kept = Vec::new();
    for (path, value) in flatten_leaves(overlay) {
        match pin(&path) {
            Some(pin) => refused.push(PinnedLeaf { key: path, pin }),
            None => kept.push((path, value)),
        }
    }
    (nest_leaves(kept), refused)
}

/// How much of a path a drop verb takes out of a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeafScope {
    /// Exactly the leaf at the path: `config.reset` and `config.pop`.
    Exact,
    /// The leaf and every leaf under it: `config.unset`, which removes a map
    /// entry a flat write can only add to.
    Subtree,
}

/// Take `path` (or the subtree under it) out of one layer's leaves, and answer
/// whether anything was there.
fn remove_leaves(
    leaves: &mut serde_json::Map<String, Value>,
    path: &str,
    scope: LeafScope,
) -> bool {
    match scope {
        LeafScope::Exact => leaves.shift_remove(path).is_some(),
        LeafScope::Subtree => {
            let child_prefix = format!("{path}.");
            let doomed: Vec<String> = leaves
                .keys()
                .filter(|key| key.as_str() == path || key.starts_with(&child_prefix))
                .cloned()
                .collect();
            for key in &doomed {
                leaves.shift_remove(key);
            }
            !doomed.is_empty()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use strum::IntoEnumIterator;

    /// A human's own line pins the leaf, and `config.save` is refused with
    /// the file and the line to change instead.
    #[test]
    fn a_human_lua_line_refuses_a_save_of_that_leaf() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"chat": {"show_thinking": true}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(3),
            },
        );

        let (accepted, refused) = store.split_pinned(json!({"chat": {"show_thinking": false}}));

        assert_eq!(
            accepted,
            json!({}),
            "a refused leaf must not reach the merge"
        );
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].key, "chat.show_thinking");
        assert_eq!(refused[0].pin.source, "lua");
        assert_eq!(refused[0].pin.file.as_deref(), Some("/config/init.lua"));
        assert_eq!(refused[0].pin.line, Some(3));
    }

    /// The `:set` case. An ephemeral write takes the value and the provenance
    /// row, but the human's line still runs at the next boot — so the pin
    /// stands and a later `config.save` is still refused.
    ///
    /// Without a pin record of its own, one `:set` would open the key: the
    /// save would be accepted, written to `settings.json`, and then shadowed
    /// at the next boot with nothing to show for it.
    #[test]
    fn an_ephemeral_set_takes_the_value_and_leaves_the_pin_standing() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"chat": {"show_thinking": true}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(3),
            },
        );

        store.merge(json!({"chat": {"show_thinking": false}}), SourceTag::Rpc);

        assert_eq!(
            store.value()["chat"]["show_thinking"],
            json!(false),
            "the runtime knob wins the value"
        );
        assert_eq!(
            store
                .provenance()
                .get("chat.show_thinking")
                .map(SourceTag::short),
            Some("rpc"),
            "and it is the last writer"
        );
        let (_, refused) = store.split_pinned(json!({"chat": {"show_thinking": false}}));
        assert_eq!(refused.len(), 1, "the pin survives the runtime knob");
        assert_eq!(refused[0].pin.line, Some(3));
    }

    /// The origin row and the refusal are one projection, and a runtime knob
    /// is what would part them.
    ///
    /// A settings UI renders its lock from the origin and offers the save the
    /// refusal answers. If the origin read the last writer, the routine `:set`
    /// would report the leaf unpinned and file-less: the control would unlock,
    /// invite the save, and the save would come back refused by a file the
    /// control never named.
    #[test]
    fn the_origin_names_the_pin_the_refusal_names_after_a_runtime_set() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"chat": {"show_thinking": true}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(3),
            },
        );

        store.merge(json!({"chat": {"show_thinking": false}}), SourceTag::Rpc);

        let origin = store.origin("chat.show_thinking");
        let (_, refused) = store.split_pinned(json!({"chat": {"show_thinking": false}}));
        assert_eq!(refused.len(), 1, "the pin survives the runtime knob");
        assert!(
            origin.pinned,
            "so the origin must report the lock: {origin:?}"
        );
        assert_eq!(
            origin.origin, refused[0].pin,
            "and it must name the same line the refusal names"
        );
    }

    /// A leaf no layer above `settings.json` holds reports its last writer,
    /// so the UI can say where the value in the box came from.
    #[test]
    fn an_unpinned_leaf_reports_the_layer_that_wrote_it() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "sonnet"}}), SourceTag::Settings);

        let origin = store.origin("chat.model");

        assert!(!origin.pinned, "{origin:?}");
        assert_eq!(origin.origin.source, "settings");
        assert_eq!(origin.origin.file, None);
    }

    /// The layer that makes the settings UI usable: a plugin declares a
    /// default, and the user can still save over it.
    #[test]
    fn a_plugin_default_leaves_the_leaf_savable() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"alpha": {"retries": 3}}),
            SourceTag::PluginDefault {
                plugin: "alpha".to_string(),
                file: "/plugins/alpha/init.lua".to_string(),
                line: Some(9),
            },
        );

        let (accepted, refused) = store.split_pinned(json!({"alpha": {"retries": 5}}));

        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(accepted, json!({"alpha": {"retries": 5}}));
    }

    /// A refusal is per leaf, not per request: the sibling the user also
    /// changed still saves, and only the pinned leaf comes back refused.
    #[test]
    fn a_pinned_leaf_does_not_refuse_its_siblings() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"chat": {"show_thinking": true}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(3),
            },
        );

        let (accepted, refused) =
            store.split_pinned(json!({"chat": {"show_thinking": false, "model": "sonnet"}}));

        assert_eq!(accepted, json!({"chat": {"model": "sonnet"}}));
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].key, "chat.show_thinking");
    }

    /// A save changes the LIVE value, and it does so for exactly the leaves
    /// it accepts.
    ///
    /// `Rpc` outranks `Settings`, so without the drop the saved leaf lost to
    /// the `:set` sitting above it: the file changed, the value did not, and
    /// the save appeared only at the next boot. The drop and the refusal come
    /// from one walk, which is what keeps a refused leaf holding the value
    /// the user raised over the pinned line.
    #[test]
    fn a_save_takes_back_the_leaves_it_accepts_and_no_others() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"chat": {"show_thinking": true}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(3),
            },
        );
        store.merge(
            json!({"chat": {"show_thinking": false, "model": "scratch", "theme": "scratch"}}),
            SourceTag::Rpc,
        );

        let saved = store.save(
            json!({"chat": {"show_thinking": true, "model": "saved"}}),
            &|_| None,
        );

        assert_eq!(saved.accepted, json!({"chat": {"model": "saved"}}));
        assert_eq!(saved.refused.len(), 1, "{:?}", saved.refused);
        assert_eq!(saved.refused[0].key, "chat.show_thinking");
        assert!(saved.withheld.is_empty());

        assert_eq!(
            store.value()["chat"]["model"],
            json!("saved"),
            "the saved leaf is live at once, not at the next boot"
        );
        assert_eq!(store.origin("chat.model").origin.source, "settings");
        assert_eq!(
            store.value()["chat"]["show_thinking"],
            json!(false),
            "a refused leaf keeps the value the runtime knob raised: a save the \
             daemon would not take must not undo a `:set`"
        );
        assert_eq!(
            store.value()["chat"]["theme"],
            json!("scratch"),
            "and a scratch value on a leaf nobody saved stands"
        );
        assert_eq!(store.origin("chat.theme").origin.source, "rpc");
    }

    /// The drop is a drop of the ephemeral layer, not a delete of the leaf: a
    /// `config.pop` after a save still reveals what sits under it.
    #[test]
    fn a_save_leaves_the_layers_under_it_intact() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "compiled"}}), SourceTag::Default);
        store.merge(json!({"chat": {"model": "scratch"}}), SourceTag::Rpc);

        store.save(json!({"chat": {"model": "saved"}}), &|_| None);
        assert_eq!(store.value()["chat"]["model"], json!("saved"));

        assert!(matches!(store.pop("chat.model"), LayerDrop::Dropped(_)));
        assert_eq!(
            store.value()["chat"]["model"],
            json!("compiled"),
            "the layer under the save is still there to reveal"
        );
    }

    #[test]
    fn a_load_store_accepts_location_keys() {
        let mut store = ConfigStore::for_load();
        let withheld = store.merge(json!({"runtimepath": ["/x"], "chat": {}}), SourceTag::Rpc);
        assert!(withheld.is_empty());
        assert_eq!(store.value()["runtimepath"], json!(["/x"]));
    }

    /// Derived from the const, never a hand list: every location key merged
    /// into a runtime store is absent from the value and present in the
    /// report.
    #[test]
    fn a_runtime_store_withholds_every_location_key() {
        let mut store = ConfigStore::runtime();
        let mut overlay = serde_json::Map::new();
        for key in LOCATION_CONFIG_KEYS {
            overlay.insert(key.to_string(), json!("/somewhere"));
        }
        overlay.insert("default_kiln".to_string(), json!("notes"));

        let withheld = store.merge(Value::Object(overlay), SourceTag::Rpc);

        for key in LOCATION_CONFIG_KEYS {
            assert!(
                withheld.contains(&key.to_string()),
                "'{key}' missing from the withheld report"
            );
            assert!(
                store.value().get(key).is_none(),
                "'{key}' reached a Withhold store"
            );
        }
        // `default_kiln` holds a NAME, not a place; it passes.
        assert_eq!(store.value()["default_kiln"], json!("notes"));
    }

    #[test]
    fn ending_the_boot_phase_strips_locations_and_flips_the_policy() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"kiln_path": "/kiln", "chat": {"show_thinking": true}}),
            SourceTag::Rpc,
        );
        store.end_boot_phase();

        assert!(store.value().get("kiln_path").is_none());
        assert_eq!(store.value()["chat"]["show_thinking"], json!(true));
        assert_eq!(
            store.merge(json!({"kiln_path": "/elsewhere"}), SourceTag::Rpc),
            vec!["kiln_path".to_string()]
        );
    }

    #[test]
    fn merge_records_leaf_provenance_per_source() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"chat": {"show_thinking": false, "show_diffs": true}}),
            SourceTag::Toml("/tmp/config.toml".into()),
        );
        store.merge(
            json!({"chat": {"show_thinking": true}}),
            SourceTag::Lua {
                file: "init.lua".into(),
                line: Some(3),
            },
        );

        assert_eq!(
            store
                .provenance()
                .get("chat.show_thinking")
                .map(SourceTag::short),
            Some("lua")
        );
        assert_eq!(
            store
                .provenance()
                .get("chat.show_diffs")
                .map(SourceTag::short),
            Some("toml")
        );
        // The sibling stands: the second write never named it.
        assert_eq!(store.value()["chat"]["show_diffs"], json!(true));
    }

    #[test]
    fn extract_ignores_unknown_keys_and_keeps_them_in_the_store() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"default_kiln": "notes", "myplugin": {"debug": true}}),
            SourceTag::Rpc,
        );
        let config = store.extract().expect("extraction must succeed");
        assert_eq!(config.default_kiln.as_deref(), Some("notes"));
        assert_eq!(store.value()["myplugin"]["debug"], json!(true));
    }

    /// The invariant `config.effective`'s `kiln_path_is_default` rests on:
    /// EVERY leaf the store's value holds has a provenance row, because
    /// `merge` is the only write door and it records as it merges. A future
    /// bypass would have to add a store method — this test is the gate that
    /// makes "no provenance row" mean "defaulted" rather than "someone
    /// forgot".
    #[test]
    fn every_leaf_in_the_store_has_a_provenance_row() {
        fn walk(value: &Value, path: &mut String, check: &mut dyn FnMut(&str)) {
            match value {
                Value::Object(map) if !map.is_empty() => {
                    for (key, child) in map {
                        let saved = path.len();
                        if !path.is_empty() {
                            path.push('.');
                        }
                        path.push_str(key);
                        walk(child, path, check);
                        path.truncate(saved);
                    }
                }
                _ => check(path),
            }
        }

        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"kiln_path": "/k", "chat": {"show_thinking": true, "model": "m"}}),
            SourceTag::Toml("/tmp/config.toml".into()),
        );
        store.merge(
            json!({"chat": {"model": "n"}, "llm": {"providers": {"a": {"endpoint": "x"}}}}),
            SourceTag::Lua {
                file: "init.lua".into(),
                line: Some(2),
            },
        );
        store.merge(
            json!({"llm": {"providers": {"b": {"endpoint": "y"}}}}),
            SourceTag::Rpc,
        );

        let value = store.value().clone();
        let mut missing = Vec::new();
        walk(&value, &mut String::new(), &mut |path| {
            if store.provenance().get(path).is_none() {
                missing.push(path.to_string());
            }
        });
        assert!(
            missing.is_empty(),
            "leaves present in the value with no provenance row: {missing:?}"
        );
    }

    // ── one write, one leaf ──────────────────────────────────────────────

    /// A dotted key and a nested table name the SAME leaf.
    ///
    /// The store used to record a dotted key as one top-level key whose name
    /// held a dot, so `llm.providers.openai.endpoint` written by a human and
    /// `llm = { providers = { openai = { endpoint = … } } }` written by a
    /// plugin were two different rows. The rank gate never saw the collision,
    /// and the plugin's value was what a reader of that path got back.
    #[test]
    fn a_plugin_cannot_take_a_leaf_a_human_wrote_under_the_other_spelling() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"llm.providers.openai.endpoint": "human"}),
            lua_line(4),
        );

        store.merge(
            json!({"llm": {"providers": {"openai": {"endpoint": "plugin"}}}}),
            plugin_default(),
        );

        assert_eq!(
            store.value()["llm"]["providers"]["openai"]["endpoint"],
            json!("human"),
            "one path is one leaf, so the rank gate must refuse the plugin: {}",
            store.value()
        );
        assert!(
            store.value().get("llm.providers.openai.endpoint").is_none(),
            "a dotted key is a path, not a key with a dot in its name: {}",
            store.value()
        );
        assert_eq!(
            store
                .provenance()
                .get("llm.providers.openai.endpoint")
                .map(SourceTag::short),
            Some("lua"),
            "and one row names the one leaf"
        );
    }

    /// An empty table sets nothing, so it names no leaf and records no row.
    #[test]
    fn an_empty_table_writes_no_leaf() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"llm": {"providers": {}}, "chat": {}}),
            SourceTag::Rpc,
        );

        assert!(
            store.provenance().is_empty(),
            "an empty table is not a value: {:?}",
            store.recorded_leaves()
        );
    }

    /// A save the merge would then drop must be REFUSED, not accepted.
    ///
    /// `Cli` outranks `Settings` and a save cannot drop it, so a saved value
    /// under it acts nowhere. The store used to accept the leaf, hand it to
    /// the caller for `settings.json`, and then let the rank gate throw it
    /// away — an `ok: true` for a write that changed nothing.
    #[test]
    fn a_save_the_rank_gate_would_drop_is_refused_and_carries_nothing_to_the_file() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "from-the-flag"}}), SourceTag::Cli);

        let saved = store.save(json!({"chat": {"model": "saved"}}), &|_| None);

        assert_eq!(
            saved.accepted,
            json!({}),
            "nothing may reach settings.json for a leaf the merge drops"
        );
        assert_eq!(saved.refused.len(), 1, "{:?}", saved.refused);
        assert_eq!(saved.refused[0].key, "chat.model");
        assert_eq!(saved.refused[0].pin.source, "cli");
        assert_eq!(
            store.value()["chat"]["model"],
            json!("from-the-flag"),
            "and the flag keeps the leaf"
        );
    }

    /// A save reaches exactly the leaves the caller named.
    ///
    /// An empty table used to be one leaf at the BRANCH path, so the save
    /// dropped the ephemeral layer's whole subtree there and destroyed a
    /// `:set` nobody named.
    #[test]
    fn a_save_of_an_empty_table_destroys_no_runtime_set() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"llm": {"providers": {"mine": {"endpoint": "scratch"}}}}),
            SourceTag::Rpc,
        );

        let saved = store.save(json!({"llm": {"providers": {}}}), &|_| None);

        assert_eq!(saved.accepted, json!({}), "an empty table names no leaf");
        assert!(saved.refused.is_empty(), "{:?}", saved.refused);
        assert_eq!(
            store.value()["llm"]["providers"]["mine"]["endpoint"],
            json!("scratch"),
            "a save must not reach a leaf the caller never named"
        );
    }

    // ── `config.unset` ───────────────────────────────────────────────────

    /// The one thing a flat write cannot say: drop a map entry.
    ///
    /// `unset` takes the whole subtree out of the layers a reset drops, so a
    /// stale provider goes and its siblings — and the layers a file restores
    /// — stand.
    #[test]
    fn unset_removes_one_provider_and_leaves_its_siblings_standing() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"llm": {"providers": {"openai": {"endpoint": "from-init"}}}}),
            lua_line(2),
        );
        store.merge(
            json!({"llm": {"providers": {
                "stale": {"endpoint": "old", "default_model": "m"},
                "keep": {"endpoint": "new"}}}}),
            SourceTag::Rpc,
        );

        let dropped = store.unset("llm.providers.stale");

        assert!(
            store.value()["llm"]["providers"].get("stale").is_none(),
            "the whole map entry must go: {}",
            store.value()["llm"]["providers"]
        );
        assert_eq!(
            store.value()["llm"]["providers"]["keep"]["endpoint"],
            json!("new"),
            "a sibling the caller did not name stands"
        );
        assert_eq!(
            store.value()["llm"]["providers"]["openai"]["endpoint"],
            json!("from-init"),
            "and so does the provider the human's file declares"
        );
        assert!(
            store
                .provenance()
                .get("llm.providers.stale.endpoint")
                .is_none(),
            "the provenance rows go with the value"
        );
        assert!(
            matches!(&dropped, LayerDrop::Dropped(sources) if sources.len() == 1
                && sources[0].short() == "rpc"),
            "{dropped:?}"
        );
    }

    /// `unset` edits no file a person wrote, so a provider `init.lua`
    /// declares survives it and the store says nothing was dropped.
    #[test]
    fn unset_leaves_a_provider_a_human_file_declares_standing() {
        let mut store = ConfigStore::runtime();
        store.merge(
            json!({"llm": {"providers": {"openai": {"endpoint": "from-init"}}}}),
            lua_line(2),
        );

        let dropped = store.unset("llm.providers.openai");

        assert!(matches!(dropped, LayerDrop::Untouched), "{dropped:?}");
        assert_eq!(
            store.value()["llm"]["providers"]["openai"]["endpoint"],
            json!("from-init")
        );
    }

    /// A plugin default helper, so the four precedence tests below name the
    /// same layer without four copies of the struct literal.
    fn plugin_default() -> SourceTag {
        SourceTag::PluginDefault {
            plugin: "alpha".to_string(),
            file: "/plugins/alpha/init.lua".to_string(),
            line: Some(9),
        }
    }

    /// The rank decides the leaf, not the call order.
    ///
    /// The boot merges `settings.json` BEFORE it evaluates `init.lua`, and a
    /// plugin's `setup()` runs DURING that evaluation — so the plugin always
    /// writes last. Under last-write-wins a plugin default silently replaced
    /// what the user saved through the settings UI.
    #[test]
    fn a_plugin_default_never_overwrites_what_the_user_saved() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"chat": {"model": "saved-by-the-user"}}),
            SourceTag::Settings,
        );

        store.merge(
            json!({"chat": {"model": "plugin-default"}}),
            plugin_default(),
        );

        assert_eq!(
            store.value()["chat"]["model"],
            json!("saved-by-the-user"),
            "a lower layer that writes later must not take the leaf"
        );
        assert_eq!(
            store.provenance().get("chat.model").map(SourceTag::short),
            Some("settings"),
            "and the provenance must name whoever the store kept"
        );
    }

    /// A layer that refuses a `config.save` must also WIN the leaf against
    /// the layer a save writes — checked on a store, for every variant the
    /// compiler knows.
    ///
    /// The refusal is only honest if the save it refused would have acted
    /// nowhere, so both halves run here: `save` answers `refused` and carries
    /// nothing to `settings.json`, and a `Settings` merge of the same leaf —
    /// which is exactly what the save would have performed — leaves the
    /// pinning layer's value and its provenance row in place.
    ///
    /// `SourceTag::pin` and `SourceTag::rank` agreeing is the enum half of
    /// this rule, and `exactly_the_layers_a_save_cannot_overwrite_refuse_one`
    /// in `provenance.rs` states it. That test reads two pure functions, so it
    /// cannot see whether the merge still consults `rank`; this one reads the
    /// merged VALUE, so it goes red when the rank gate in [`ConfigStore::apply`]
    /// does. `EnumIter` walks the variants, so a new pinning layer that loads
    /// at or below `Settings` fails here rather than shipping a refusal that
    /// takes a working write away from the user.
    #[test]
    fn every_layer_that_refuses_a_save_beats_the_settings_layer_in_the_store() {
        for tag in SourceTag::iter() {
            let Some(pin) = tag.pin() else {
                continue;
            };
            let mut store = ConfigStore::runtime();
            store.merge(json!({"chat": {"model": "from-the-pin"}}), tag.clone());

            let saved = store.save(json!({"chat": {"model": "saved"}}), &|_| None);
            assert_eq!(
                saved.accepted,
                json!({}),
                "'{}' pins the leaf, so nothing may reach settings.json",
                tag.short()
            );
            assert_eq!(
                saved
                    .refused
                    .iter()
                    .map(|leaf| (leaf.key.as_str(), leaf.pin.source))
                    .collect::<Vec<_>>(),
                vec![("chat.model", pin.source)],
                "'{}' must refuse the leaf it pins, and name itself",
                tag.short()
            );

            // What the save would have written, had the pin not refused it.
            // The pinning layer keeps the leaf, so the refusal took away a
            // write that would have changed nothing.
            store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
            assert_eq!(
                store.value()["chat"]["model"],
                json!("from-the-pin"),
                "'{}' pins a leaf the settings layer then took, so the refused \
                 save would have acted",
                tag.short()
            );
            assert_eq!(
                store.provenance().get("chat.model").map(SourceTag::short),
                Some(tag.short()),
                "and the provenance must name whoever the store kept"
            );
        }
    }

    /// Refusal is per leaf, exactly as the merge is. A plugin default that
    /// loses `model` still contributes the sibling no other layer holds,
    /// or a plugin could declare no defaults at all once a user saved one key.
    #[test]
    fn a_refused_leaf_does_not_refuse_the_siblings_of_its_own_write() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"chat": {"model": "saved-by-the-user"}}),
            SourceTag::Settings,
        );

        store.merge(
            json!({"chat": {"model": "plugin-default", "retries": 3}}),
            plugin_default(),
        );

        assert_eq!(store.value()["chat"]["model"], json!("saved-by-the-user"));
        assert_eq!(store.value()["chat"]["retries"], json!(3));
        assert_eq!(
            store.provenance().get("chat.retries").map(SourceTag::short),
            Some("plugin")
        );
    }

    /// Equal rank still overwrites: two lines of one file are ordered by the
    /// file, and the second is the one the author meant.
    #[test]
    fn a_second_write_from_the_same_layer_takes_the_leaf() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"chat": {"model": "first"}}), SourceTag::Settings);
        store.merge(json!({"chat": {"model": "second"}}), SourceTag::Settings);
        assert_eq!(store.value()["chat"]["model"], json!("second"));
    }

    /// The other direction of the same rule: a human's own line outranks the
    /// saved settings, so it takes the leaf and keeps it.
    ///
    /// **The second merge is the half that reads the rank.** At boot the Lua
    /// line writes last, so the boot order alone gives the same answer and
    /// last-write-wins would pass the first half of this test. A `config.save`
    /// writes the `Settings` layer AFTER `init.lua` was evaluated, which is
    /// the order that asks the question: the human's line must still hold the
    /// leaf.
    #[test]
    fn a_human_lua_line_takes_a_leaf_the_settings_layer_holds() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
        store.merge(json!({"chat": {"model": "from-init"}}), lua_line(2));
        assert_eq!(store.value()["chat"]["model"], json!("from-init"));
        assert_eq!(
            store.provenance().get("chat.model").map(SourceTag::short),
            Some("lua")
        );

        // What a later `config.save` merges. The line the human wrote wins it
        // on rank, not on write order.
        store.merge(
            json!({"chat": {"model": "saved-later"}}),
            SourceTag::Settings,
        );
        assert_eq!(
            store.value()["chat"]["model"],
            json!("from-init"),
            "a settings write that lands later must not take a human's line"
        );
        assert_eq!(
            store.provenance().get("chat.model").map(SourceTag::short),
            Some("lua"),
            "and the provenance must name whoever the store kept"
        );
    }

    /// A refused write leaves no trace at all — no value, no provenance row,
    /// and no pin. A pin recorded for a write that did not land would refuse
    /// a `config.save` on behalf of a value nobody can see.
    #[test]
    fn a_refused_write_records_neither_provenance_nor_a_pin() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"chat": {"model": "from-init"}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(2),
            },
        );

        // `Toml` pins and ranks BELOW `Lua`, so this write must not land.
        store.merge(
            json!({"chat": {"model": "from-toml"}}),
            SourceTag::Toml("/config/config.toml".into()),
        );

        assert_eq!(store.value()["chat"]["model"], json!("from-init"));
        let pin = store
            .pin("chat.model")
            .expect("the human's line still pins");
        assert_eq!(pin.file.as_deref(), Some("/config/init.lua"));
        assert_eq!(pin.line, Some(2), "the refused write must not move the pin");
    }

    #[test]
    fn a_non_object_overlay_is_refused_wholesale() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"default_kiln": "notes"}), SourceTag::Rpc);
        store.merge(json!("scalar"), SourceTag::Rpc);
        assert_eq!(store.value()["default_kiln"], json!("notes"));
    }

    // ── `config.reset` and `config.pop` ──────────────────────────────────

    /// A human's own line, for the drop tests.
    fn lua_line(line: u32) -> SourceTag {
        SourceTag::Lua {
            file: "/config/init.lua".to_string(),
            line: Some(line),
        }
    }

    /// `:set key&`. The ephemeral knob goes and the layer under it stands.
    #[test]
    fn a_reset_drops_the_runtime_knob_and_reveals_the_layer_under_it() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "from-init"}}), lua_line(2));
        store.merge(json!({"chat": {"model": "for-one-turn"}}), SourceTag::Rpc);
        assert_eq!(store.value()["chat"]["model"], json!("for-one-turn"));

        let dropped = store.reset("chat.model");

        assert_eq!(
            store.value()["chat"]["model"],
            json!("from-init"),
            "reset must return the leaf to what the files give"
        );
        assert_eq!(
            store.provenance().get("chat.model").map(SourceTag::short),
            Some("lua"),
            "and the provenance must name the layer that now holds it"
        );
        assert!(
            matches!(&dropped, LayerDrop::Dropped(sources) if sources.len() == 1
                && sources[0].short() == "rpc"),
            "{dropped:?}"
        );
    }

    /// `settings.json` is a file, so a reset leaves it standing.
    ///
    /// Dropping it in memory would answer with a value the next boot takes
    /// back, and deleting the leaf from the file would make a one-key undo of
    /// a session tweak destroy a durable preference. `config.save` writes
    /// that layer, and `config.save` is what unwrites it.
    #[test]
    fn a_reset_leaves_the_saved_settings_standing() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
        store.merge(json!({"chat": {"model": "for-one-turn"}}), SourceTag::Rpc);

        store.reset("chat.model");

        assert_eq!(
            store.value()["chat"]["model"],
            json!("saved"),
            "a reset must not discard what the user saved through the UI"
        );
    }

    /// The gate. A leaf in BOTH `settings.json` and `init.lua`, popped once,
    /// answers with the `settings.json` value and names that layer.
    #[test]
    fn a_pop_of_the_lua_line_reveals_the_settings_value() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
        store.merge(json!({"chat": {"model": "from-init"}}), lua_line(2));
        assert_eq!(store.value()["chat"]["model"], json!("from-init"));

        let dropped = store.pop("chat.model");

        assert_eq!(
            store.value()["chat"]["model"],
            json!("saved"),
            "the pop must reveal the layer under the one it dropped"
        );
        assert_eq!(
            store.provenance().get("chat.model").map(SourceTag::short),
            Some("settings"),
            "and the store must now say so"
        );
        assert!(
            matches!(&dropped, LayerDrop::Dropped(sources) if sources.len() == 1
                && sources[0].short() == "lua"),
            "{dropped:?}"
        );
    }

    /// A pop drops one layer per call, so repeated pops walk down the stack.
    #[test]
    fn a_pop_walks_down_one_layer_per_call() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "compiled"}}), SourceTag::Default);
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
        store.merge(json!({"chat": {"model": "from-init"}}), lua_line(2));
        store.merge(json!({"chat": {"model": "for-one-turn"}}), SourceTag::Rpc);

        for expected in ["from-init", "saved", "compiled"] {
            store.pop("chat.model");
            assert_eq!(
                store.value()["chat"]["model"],
                json!(expected),
                "the pop must reveal exactly one layer"
            );
        }

        // The compiled default is a layer too, so one more pop leaves the
        // leaf unheld — and only then is there nothing left to pop.
        assert!(matches!(store.pop("chat.model"), LayerDrop::Dropped(_)));
        assert!(
            store.value()["chat"].get("model").is_none(),
            "every layer is gone, so the store holds nothing for the leaf"
        );
        let last = store.pop("chat.model");
        assert!(
            matches!(last, LayerDrop::Untouched),
            "nothing is left to pop: {last:?}"
        );
    }

    /// A write the rank gate dropped is still retained, so popping the layer
    /// above it reveals it.
    ///
    /// The plugin default below never landed — `settings.json` already held
    /// the leaf — and a store that kept only what landed would answer the pop
    /// with the compiled default instead of the plugin's.
    #[test]
    fn a_pop_reveals_a_write_the_rank_gate_refused() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
        store.merge(
            json!({"chat": {"model": "plugin-default"}}),
            plugin_default(),
        );
        assert_eq!(store.value()["chat"]["model"], json!("saved"));

        store.pop("chat.model");

        assert_eq!(
            store.value()["chat"]["model"],
            json!("plugin-default"),
            "the refused write is the layer under the one that was popped"
        );
    }

    /// A leaf no layer holds has nothing to drop, and the store says so
    /// rather than reporting a change it did not make.
    #[test]
    fn a_drop_of_a_leaf_no_layer_holds_reports_nothing() {
        let mut store = ConfigStore::runtime();
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);

        assert!(matches!(store.pop("chat.retries"), LayerDrop::Untouched));
        assert!(matches!(store.reset("chat.model"), LayerDrop::Untouched));
        assert_eq!(store.value()["chat"]["model"], json!("saved"));
    }

    /// The rebuild IS the merge rule, so a popped store equals a store that
    /// merged the same layers without the popped write.
    ///
    /// This is the property the re-merge was chosen for: a per-leaf undo
    /// stack would be a second copy of the layer order, free to drift from
    /// `ConfigStore::merge`.
    #[test]
    fn a_pop_leaves_exactly_what_the_same_layers_merged_without_it_give() {
        let mut popped = ConfigStore::runtime();
        popped.merge(
            json!({"chat": {"model": "saved", "retries": 1}}),
            SourceTag::Settings,
        );
        popped.merge(json!({"chat": {"model": "from-init"}}), lua_line(2));
        popped.merge(json!({"chat": {"retries": 9}}), SourceTag::Rpc);
        popped.pop("chat.model");

        let mut merged = ConfigStore::runtime();
        merged.merge(
            json!({"chat": {"model": "saved", "retries": 1}}),
            SourceTag::Settings,
        );
        merged.merge(json!({}), lua_line(2));
        merged.merge(json!({"chat": {"retries": 9}}), SourceTag::Rpc);

        assert_eq!(popped.value(), merged.value());
        assert_eq!(
            popped
                .recorded_leaves()
                .iter()
                .map(|leaf| (leaf.to_string(), popped.origin(leaf)))
                .collect::<Vec<_>>(),
            merged
                .recorded_leaves()
                .iter()
                .map(|leaf| (leaf.to_string(), merged.origin(leaf)))
                .collect::<Vec<_>>(),
            "the rebuild must reproduce the provenance and the pins too"
        );
    }

    /// The keys that name where the daemon acts are withheld from `&` and
    /// `^` under the runtime policy, exactly as they are from a merge.
    ///
    /// A pop is a write door: it changes what the store holds. A caller that
    /// could pop `runtimepath` would re-point the trees the daemon reads code
    /// from without the floor ever seeing a path.
    #[test]
    fn a_drop_of_a_location_key_is_withheld_at_runtime() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"kiln_path": "/a"}), SourceTag::Settings);
        store.merge(json!({"kiln_path": "/b"}), lua_line(2));
        store.end_boot_phase();

        assert!(matches!(store.pop("kiln_path"), LayerDrop::Withheld));
        assert!(matches!(store.reset("kiln_path"), LayerDrop::Withheld));
    }

    /// A drop rebuilds the store, and the rebuild must not put a location key
    /// back into the plugin-visible value.
    ///
    /// The boot phase ACCEPTS `kiln_path`, so the retained layer holds it.
    /// `end_boot_phase` drops it from the value; a replay that skipped the
    /// location policy would merge it straight back in, and one `:set key^`
    /// would hand every plugin the directories the daemon reads.
    #[test]
    fn a_drop_does_not_replay_a_location_key_back_into_the_value() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"kiln_path": "/notes", "chat": {"model": "saved"}}),
            SourceTag::Settings,
        );
        store.merge(json!({"chat": {"model": "from-init"}}), lua_line(2));
        store.end_boot_phase();
        assert!(store.value().get("kiln_path").is_none());

        store.pop("chat.model");

        assert!(
            store.value().get("kiln_path").is_none(),
            "the replay must apply the runtime location policy: {}",
            store.value()
        );
        assert_eq!(store.value()["chat"]["model"], json!("saved"));
        assert_eq!(
            store.provenance().get("kiln_path").map(SourceTag::short),
            Some("settings"),
            "and it must leave the row that says a boot layer configured the kiln"
        );
    }

    /// The gate on every client's kiln: a drop must not erase the provenance
    /// row a boot layer left on a location key.
    ///
    /// `config.effective` derives `kiln_path_is_default` from the presence of
    /// that row, and a `cru` client that reads `true` substitutes its OWN
    /// working directory. The replay used to clear the provenance wholesale
    /// and then withhold every location key from the layers that carried them,
    /// so one `:set chat.model&` re-pointed every client's notes and sessions
    /// at the client's cwd, and nothing logged it.
    ///
    /// Three doors, because all three rebuild: `config.reset`, `config.pop`,
    /// and a `config.save` that drops an `Rpc` hold.
    #[test]
    fn a_drop_keeps_the_provenance_a_boot_layer_left_on_every_location_key() {
        /// A boot store that configured every location key from `init.lua`,
        /// then left the boot phase with an ephemeral knob on `chat.model`.
        fn booted() -> ConfigStore {
            let mut store = ConfigStore::for_load();
            let mut overlay = serde_json::Map::new();
            for key in LOCATION_CONFIG_KEYS {
                overlay.insert(key.to_string(), json!("/configured"));
            }
            store.merge(Value::Object(overlay), lua_line(2));
            store.merge(json!({"chat": {"model": "compiled"}}), SourceTag::Default);
            store.end_boot_phase();
            store.merge(json!({"chat": {"model": "scratch"}}), SourceTag::Rpc);
            store
        }

        /// One drop door: its name for the assertion message, and the call
        /// that performs it.
        type Door = (&'static str, fn(&mut ConfigStore));

        let doors: [Door; 3] = [
            ("reset", |store| {
                assert!(
                    matches!(store.reset("chat.model"), LayerDrop::Dropped(_)),
                    "the reset must drop the ephemeral layer, or this proves nothing"
                );
            }),
            ("pop", |store| {
                assert!(
                    matches!(store.pop("chat.model"), LayerDrop::Dropped(_)),
                    "the pop must drop a layer, or this proves nothing"
                );
            }),
            ("save", |store| {
                let saved = store.save(json!({"chat": {"model": "saved"}}), &|_| None);
                assert_eq!(
                    saved.accepted,
                    json!({"chat": {"model": "saved"}}),
                    "the save must take the leaf the Rpc layer held, or this proves nothing"
                );
            }),
        ];

        for (door, drop) in doors {
            let mut store = booted();
            for key in LOCATION_CONFIG_KEYS {
                assert_eq!(
                    store.provenance().get(key).map(SourceTag::short),
                    Some("lua"),
                    "the boot layer must own '{key}' before the {door}"
                );
            }

            drop(&mut store);

            for key in LOCATION_CONFIG_KEYS {
                assert_eq!(
                    store.provenance().get(key).map(SourceTag::short),
                    Some("lua"),
                    "the {door} erased the provenance row on '{key}', so every client \
                     writes into its own cwd instead of the configured location"
                );
                assert_eq!(
                    store.pin(key).map(|pin| pin.source),
                    Some("lua"),
                    "the {door} erased the pin on '{key}', so a save could write over \
                     the line that configured it"
                );
                assert!(
                    store.value().get(key).is_none(),
                    "the {door} put '{key}' back into the plugin-visible value: {}",
                    store.value()
                );
            }
        }
    }

    /// A layer replays under its OWN policy, and a runtime layer's own policy
    /// is `Withhold` — so a `config.set` of a location key stays refused, and
    /// a later rebuild cannot let it land.
    ///
    /// The other half of the per-layer policy. Restoring the boot layer's
    /// location keys must not restore a runtime write's: the socket has no
    /// authentication, and these keys say where the daemon acts.
    #[test]
    fn a_runtime_write_of_a_location_key_stays_refused_across_a_rebuild() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"kiln_path": "/configured"}), lua_line(2));
        store.merge(json!({"chat": {"model": "compiled"}}), SourceTag::Default);
        store.end_boot_phase();

        let withheld = store.merge(
            json!({"kiln_path": "/attacker", "chat": {"model": "scratch"}}),
            SourceTag::Rpc,
        );
        assert_eq!(withheld, vec!["kiln_path".to_string()]);

        assert!(matches!(store.pop("chat.model"), LayerDrop::Dropped(_)));

        assert!(
            store.value().get("kiln_path").is_none(),
            "the replay let a runtime write name a location: {}",
            store.value()
        );
        assert_eq!(
            store.provenance().get("kiln_path").map(SourceTag::short),
            Some("lua"),
            "and the runtime write must not take authorship of the leaf either"
        );
    }

    /// A pop of a leaf under a location key is withheld too: the policy
    /// classifies the top-level key, and the leaves under it are its parts.
    #[test]
    fn a_drop_under_a_location_key_is_withheld_at_runtime() {
        let mut store = ConfigStore::runtime();
        assert!(matches!(store.pop("kilns.notes"), LayerDrop::Withheld));
    }
}
