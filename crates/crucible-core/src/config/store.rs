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
use super::merge::{deep_merge_traced, REPLACE_MARKER};
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

/// What [`ConfigStore::reset`] or [`ConfigStore::pop`] did to one leaf.
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
    /// Always an object: the overlay as the caller offered it. The location
    /// policy is applied at every replay rather than here, so a store that
    /// has left the boot phase stops merging keys it accepted during it.
    overlay: Value,
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
    /// of the layer order — `SourceTag::rank`, the wholesale-replace rule and
    /// the pin rule all over again — and the two copies would disagree the
    /// first time one of them changed.
    ///
    /// The list grows by one entry per merge and nothing prunes it. Every
    /// writer is a boot step, a `cru` command or a keystroke, so the rate is
    /// a handful of small objects per session. Merging two adjacent
    /// same-source layers would bound it, but the merge of two overlays is
    /// not the merge of an overlay into a store — `REPLACE_MARKER` behaves
    /// differently in the two — so that shortcut would be the second rule
    /// this design exists to avoid.
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

    /// Deep-merge `overlay` into the store, recording provenance, and return
    /// the top-level location keys the policy withheld (empty under
    /// [`LocationPolicy::Accept`]).
    ///
    /// The merge is per-leaf and ranked: a leaf a higher layer already holds
    /// keeps its value and its provenance, and the write lands on every other
    /// leaf of the same overlay. See [`SourceTag::rank`] for the order.
    ///
    /// A non-object overlay is refused wholesale: the store's value is always
    /// an object, and no caller has a scalar to contribute at the root.
    pub fn merge(&mut self, overlay: Value, source: SourceTag) -> Vec<String> {
        let Value::Object(map) = overlay else {
            return Vec::new();
        };
        // Retain the layer before it lands, so `reset` and `pop` can replay
        // the store without part of it.
        self.layers.push(Layer {
            source: source.clone(),
            overlay: Value::Object(map.clone()),
        });
        self.apply(map, source)
    }

    /// Land one layer on the value, the provenance and the pins, and answer
    /// with the location keys the policy withheld.
    ///
    /// The half of [`Self::merge`] a replay repeats, the location policy
    /// included: a store that has left the boot phase stops merging the keys
    /// it once accepted, so a replay after `end_boot_phase` cannot put a
    /// location back into the plugin-visible value. It records no layer of
    /// its own — [`Self::rebuild`] walks the layers it already has, and a
    /// second push would double them on every drop.
    fn apply(&mut self, mut map: serde_json::Map<String, Value>, source: SourceTag) -> Vec<String> {
        let mut withheld = Vec::new();
        if self.location_policy == LocationPolicy::Withhold {
            for key in LOCATION_CONFIG_KEYS {
                if map.remove(key).is_some() {
                    withheld.push(key.to_string());
                }
            }
        }

        let provenance = &mut self.provenance;
        let pins = &mut self.pins;
        // Only a pinning source may write or clear a pin. An ephemeral `:set`
        // replaces the value and the provenance row, but the line that wrote
        // the leaf still runs at the next boot, so the pin outlives it.
        let pinning = source.pin().is_some();
        let rank = source.rank();
        deep_merge_traced(
            &mut self.value,
            Value::Object(map),
            &mut String::new(),
            &mut |path, landed| {
                // Layer precedence, decided per leaf by `SourceTag::rank`
                // rather than by the order the layers happen to merge in.
                // The order inverts the rule: `settings.json` merges before
                // `init.lua` is evaluated, and a plugin writes DURING that
                // evaluation — so the plugin always writes last, and
                // last-write-wins handed it the leaf the user saved. Equal
                // rank still writes: two lines of one file are ordered by the
                // file, and the second is the one the author meant.
                if provenance
                    .max_rank_at_or_under(path)
                    .is_some_and(|held| rank < held)
                {
                    return false;
                }
                // A wholesale write invalidates whatever provenance the old
                // subtree carried, then records the new leaves.
                provenance.clear_prefix(path);
                record_leaves(provenance, path, landed, &source);
                if pinning {
                    pins.clear_prefix(path);
                    record_leaves(pins, path, landed, &source);
                }
                true
            },
        );
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
        let indices: Vec<usize> = self
            .layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.source.reset_drops() && layer_holds(layer, path))
            .map(|(index, _)| index)
            .collect();
        self.drop_from_layers(&indices, path)
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
            .filter(|(_, layer)| layer_holds(layer, path))
            .max_by_key(|(index, layer)| (layer.source.rank(), *index))
            .map(|(index, _)| index);
        self.drop_from_layers(top.as_slice(), path)
    }

    /// Take `path` out of the named layers and replay what is left.
    fn drop_from_layers(&mut self, indices: &[usize], path: &str) -> LayerDrop {
        let mut dropped = Vec::new();
        for &index in indices {
            let Some(layer) = self.layers.get_mut(index) else {
                continue;
            };
            if let Value::Object(map) = &mut layer.overlay {
                if remove_leaf(map, path) {
                    dropped.push(layer.source.clone());
                }
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
    fn rebuild(&mut self) {
        let layers = std::mem::take(&mut self.layers);
        self.value = Value::Object(serde_json::Map::new());
        self.provenance = ProvenanceMap::new();
        self.pins = ProvenanceMap::new();
        for layer in &layers {
            if let Value::Object(map) = &layer.overlay {
                let _ = self.apply(map.clone(), layer.source.clone());
            }
        }
        self.layers = layers;
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
    /// One walk answers both halves, so they cannot disagree about a leaf.
    /// The walk follows the merge's own shape: an object recurses key by key,
    /// while an array, a scalar and a [`REPLACE_MARKER`] table are each one
    /// leaf, because that is what the merge writes wholesale.
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
    /// **The refusal and the drop are one walk, so they cannot disagree.** A
    /// leaf a pin refuses is neither merged nor dropped, which is what keeps
    /// a refused save from undoing the `:set` the user raised over a pinned
    /// line. Two walks would answer "which leaves am I saving" twice, and the
    /// second answer would clear a value nothing replaced.
    ///
    /// `also_pinned` carries the pins the store cannot see: the daemon's
    /// state overlay (`llm.json`) holds leaves no layer here carries.
    pub fn save(
        &mut self,
        overlay: Value,
        also_pinned: &dyn Fn(&str) -> Option<SourceOrigin>,
    ) -> SavedSettings {
        let mut refused = Vec::new();
        let mut leaves: Vec<String> = Vec::new();
        let accepted = split_value(
            &mut String::new(),
            overlay,
            &|path| self.pin(path).or_else(|| also_pinned(path)),
            &mut refused,
            &mut |leaf| leaves.push(leaf.to_string()),
        )
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

        let mut dropped = false;
        for layer in self
            .layers
            .iter_mut()
            .filter(|layer| layer.source.reset_drops())
        {
            let Value::Object(map) = &mut layer.overlay else {
                continue;
            };
            for leaf in &leaves {
                dropped |= remove_leaf(map, leaf);
            }
        }
        if dropped {
            // Before the merge, not after: the rank gate reads the provenance
            // the store holds now, and a stale `Rpc` row would refuse the very
            // write this drop was made for.
            self.rebuild();
        }
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
    /// The union, not the provenance map alone. A wholesale write clears the
    /// provenance rows under the path it replaces and records only what it
    /// wrote, while the pin survives it — so a leaf can still be refused by a
    /// line after its provenance row is gone. A listing built from
    /// `provenance` would drop exactly the leaves a UI must render locked.
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
/// carries. [`ConfigStore::save`] weighs both hands and walks the same
/// `split_value`, so the refusal cannot mean one thing here and another
/// there.
pub fn split_pinned_by(
    overlay: Value,
    pin: &dyn Fn(&str) -> Option<SourceOrigin>,
) -> (Value, Vec<PinnedLeaf>) {
    let mut refused = Vec::new();
    let accepted = split_value(&mut String::new(), overlay, pin, &mut refused, &mut |_| {})
        .unwrap_or(Value::Object(serde_json::Map::new()));
    (accepted, refused)
}

/// The recursive half of [`split_pinned_by`]. `None` means the whole subtree
/// was refused and must not reach the merge.
///
/// `accepted` is called with the path of every leaf that survives the pins,
/// so a caller that must act on those leaves reads them from this walk
/// instead of walking the result again. [`ConfigStore::save`] is that caller:
/// it drops the ephemeral hold on exactly the leaves it is about to merge.
fn split_value(
    path: &mut String,
    value: Value,
    pin: &dyn Fn(&str) -> Option<SourceOrigin>,
    refused: &mut Vec<PinnedLeaf>,
    accepted: &mut dyn FnMut(&str),
) -> Option<Value> {
    // The root is always walked: the overlay's own object is not a leaf, and
    // no leaf path names it.
    let recurses = match &value {
        Value::Object(map) => {
            path.is_empty() || (!map.is_empty() && !map.contains_key(REPLACE_MARKER))
        }
        _ => false,
    };
    if !recurses {
        return match pin(path) {
            Some(pin) => {
                refused.push(PinnedLeaf {
                    key: path.clone(),
                    pin,
                });
                None
            }
            None => {
                accepted(path);
                Some(value)
            }
        };
    }

    let Value::Object(map) = value else {
        unreachable!("only an object recurses");
    };
    let at_root = path.is_empty();
    let mut kept = serde_json::Map::new();
    for (key, child) in map {
        let saved = path.len();
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(&key);
        if let Some(child) = split_value(path, child, pin, refused, accepted) {
            kept.insert(key, child);
        }
        path.truncate(saved);
    }
    // An empty branch is dropped rather than merged: every leaf under it
    // was refused, and merging `{}` would record a provenance row for a
    // value the caller never got to write. The root stays, because the
    // merge needs an object even when it carries nothing.
    (at_root || !kept.is_empty()).then_some(Value::Object(kept))
}

/// Whether one retained layer writes anything at `path`.
fn layer_holds(layer: &Layer, path: &str) -> bool {
    matches!(&layer.overlay, Value::Object(map) if leaf_at(map, path).is_some())
}

/// The value one overlay writes at a dot-joined path, if it writes one.
///
/// The literal name comes first, for the same reason `config.origin` reads it
/// first: a plugin owns free-form keys, and `config.set { "myplugin.debug":
/// true }` writes ONE top-level key whose name holds a dot.
fn leaf_at<'a>(map: &'a serde_json::Map<String, Value>, path: &str) -> Option<&'a Value> {
    if let Some(value) = map.get(path) {
        return Some(value);
    }
    let (head, rest) = path.split_once('.')?;
    match map.get(head)? {
        Value::Object(child) => leaf_at(child, rest),
        _ => None,
    }
}

/// Take `path` out of one overlay, and answer whether it was there.
///
/// A branch the removal empties goes too: an empty object is a leaf to
/// [`record_leaves`], so leaving `{}` behind would replay as a write of `{}`
/// over the value the drop was meant to reveal.
fn remove_leaf(map: &mut serde_json::Map<String, Value>, path: &str) -> bool {
    if map.shift_remove(path).is_some() {
        return true;
    }
    let Some((head, rest)) = path.split_once('.') else {
        return false;
    };
    let Some(Value::Object(child)) = map.get_mut(head) else {
        return false;
    };
    if !remove_leaf(child, rest) {
        return false;
    }
    if child.is_empty() {
        map.shift_remove(head);
    }
    true
}

/// Record one provenance entry per leaf of `value` under `path`.
///
/// An array is one leaf (arrays replace wholesale); an empty object is one
/// leaf too, because it was written as a value.
fn record_leaves(provenance: &mut ProvenanceMap, path: &str, value: &Value, source: &SourceTag) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                record_leaves(provenance, &child_path, child, source);
            }
        }
        _ => {
            if !path.is_empty() {
                provenance.set(path, source.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        // The sibling survived the deep merge.
        assert_eq!(store.value()["chat"]["show_diffs"], json!(true));
    }

    #[test]
    fn a_replacement_clears_the_ghost_provenance_of_removed_keys() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"llm": {"providers": {"a": {"endpoint": "x"}, "b": {"endpoint": "y"}}}}),
            SourceTag::Toml("/tmp/config.toml".into()),
        );
        store.merge(
            json!({"llm": {"providers": {"__replace": true, "a": {"endpoint": "x"}}}}),
            SourceTag::Rpc,
        );

        assert!(
            store.provenance().get("llm.providers.b.endpoint").is_none(),
            "a removed provider kept a ghost provenance row"
        );
        assert_eq!(
            store.provenance().get("llm.providers.a.endpoint"),
            Some(&SourceTag::Rpc)
        );
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
            json!({"llm": {"providers": {"__replace": true, "b": {"endpoint": "y"}}}}),
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

    /// The other direction of the same rule: a human's own line loads after
    /// the saved settings and outranks them, so it takes the leaf.
    #[test]
    fn a_human_lua_line_takes_a_leaf_the_settings_layer_holds() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"chat": {"model": "saved"}}), SourceTag::Settings);
        store.merge(
            json!({"chat": {"model": "from-init"}}),
            SourceTag::Lua {
                file: "/config/init.lua".to_string(),
                line: Some(2),
            },
        );
        assert_eq!(store.value()["chat"]["model"], json!("from-init"));
        assert_eq!(
            store.provenance().get("chat.model").map(SourceTag::short),
            Some("lua")
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

    /// A wholesale replacement is one write over a whole subtree, so the rank
    /// it must beat is the highest rank under the path — not the (absent) row
    /// on the branch itself. Otherwise `__replace` would be the door around
    /// the layer order.
    #[test]
    fn a_lower_layer_replacement_cannot_wipe_a_higher_layer_subtree() {
        let mut store = ConfigStore::for_load();
        store.merge(
            json!({"llm": {"providers": {"a": {"endpoint": "saved"}}}}),
            SourceTag::Settings,
        );

        store.merge(
            json!({"llm": {"providers": {"__replace": true, "b": {"endpoint": "plugin"}}}}),
            plugin_default(),
        );

        assert_eq!(
            store.value()["llm"]["providers"]["a"]["endpoint"],
            json!("saved"),
            "a plugin default must not replace a subtree the user saved"
        );
        assert!(
            store.value()["llm"]["providers"].get("b").is_none(),
            "and the replacement must not land in part either"
        );
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
    }

    /// A pop of a leaf under a location key is withheld too: the policy
    /// classifies the top-level key, and the leaves under it are its parts.
    #[test]
    fn a_drop_under_a_location_key_is_withheld_at_runtime() {
        let mut store = ConfigStore::runtime();
        assert!(matches!(store.pop("kilns.notes"), LayerDrop::Withheld));
    }
}
