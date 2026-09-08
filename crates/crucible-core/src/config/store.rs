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

/// Whether a merge may write the keys that name a filesystem location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationPolicy {
    /// Load time: location keys are legitimate config authorship.
    Accept,
    /// Daemon runtime: location keys are withheld and reported.
    Withhold,
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
}

impl ConfigStore {
    /// A boot-phase store: location keys are accepted.
    pub fn for_load() -> Self {
        Self {
            value: Value::Object(serde_json::Map::new()),
            provenance: ProvenanceMap::new(),
            pins: ProvenanceMap::new(),
            location_policy: LocationPolicy::Withhold,
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
        let Value::Object(mut map) = overlay else {
            return Vec::new();
        };

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
/// Free rather than a method, because the store is not the only source of a
/// pin. The daemon's state overlay (`llm.json`) contributes leaves the store
/// never holds, and `config.save` has to refuse those by the same walk: two
/// walks would disagree about where one leaf ends.
pub fn split_pinned_by(
    overlay: Value,
    pin: &dyn Fn(&str) -> Option<SourceOrigin>,
) -> (Value, Vec<PinnedLeaf>) {
    let mut refused = Vec::new();
    let accepted = split_value(&mut String::new(), overlay, pin, &mut refused)
        .unwrap_or(Value::Object(serde_json::Map::new()));
    (accepted, refused)
}

/// The recursive half of [`split_pinned_by`]. `None` means the whole subtree
/// was refused and must not reach the merge.
fn split_value(
    path: &mut String,
    value: Value,
    pin: &dyn Fn(&str) -> Option<SourceOrigin>,
    refused: &mut Vec<PinnedLeaf>,
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
            None => Some(value),
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
        if let Some(child) = split_value(path, child, pin, refused) {
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
}
