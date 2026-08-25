//! The one config store: a JSON object, its provenance, and the phase flag.
//!
//! One store serves two phases instead of two stores in two VMs (§9 of the
//! execution plan). During the boot evaluation of `init.lua` the policy is
//! [`LocationPolicy::Accept`]: location keys are legitimate config
//! authorship. After the daemon extracts the config and freezes the location
//! slice, the policy is [`LocationPolicy::Withhold`]: location keys are
//! withheld from every merge and reported, because the RPC socket has no
//! authentication and these keys answer *where the daemon acts*.

use serde_json::Value;

use super::config::{CliAppConfig, ConfigError, LOCATION_CONFIG_KEYS};
use super::merge::deep_merge_traced;
use super::provenance::{ProvenanceMap, SourceTag};

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
    location_policy: LocationPolicy,
}

impl ConfigStore {
    /// A boot-phase store: location keys are accepted.
    pub fn for_load() -> Self {
        Self {
            value: Value::Object(serde_json::Map::new()),
            provenance: ProvenanceMap::new(),
            location_policy: LocationPolicy::Withhold,
        }
        .with_policy(LocationPolicy::Accept)
    }

    /// A runtime store: location keys are withheld and reported.
    pub fn runtime() -> Self {
        Self {
            value: Value::Object(serde_json::Map::new()),
            provenance: ProvenanceMap::new(),
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
        deep_merge_traced(
            &mut self.value,
            Value::Object(map),
            &mut String::new(),
            &mut |path, landed| {
                // A wholesale write invalidates whatever provenance the old
                // subtree carried, then records the new leaves.
                provenance.clear_prefix(path);
                record_leaves(provenance, path, landed, &source);
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

    /// The merged value. Always an object.
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Per-leaf provenance for the merged value.
    pub fn provenance(&self) -> &ProvenanceMap {
        &self.provenance
    }

    /// Extract the typed config from the merged value.
    ///
    /// Unknown keys are ignored by extraction and stay in the store — plugins
    /// own free-form `plugins.<name>` keys.
    pub fn extract(&self) -> Result<CliAppConfig, ConfigError> {
        serde_json::from_value(self.value.clone()).map_err(ConfigError::Serialization)
    }
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
            store.provenance().get("chat.show_thinking").map(SourceTag::short),
            Some("lua")
        );
        assert_eq!(
            store.provenance().get("chat.show_diffs").map(SourceTag::short),
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

    #[test]
    fn a_non_object_overlay_is_refused_wholesale() {
        let mut store = ConfigStore::for_load();
        store.merge(json!({"default_kiln": "notes"}), SourceTag::Rpc);
        store.merge(json!("scalar"), SourceTag::Rpc);
        assert_eq!(store.value()["default_kiln"], json!("notes"));
    }
}
