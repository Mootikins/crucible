//! `<data_home>/llm.json` — the LLM provider selection the daemon was told.
//!
//! # Why a provider selection is state
//!
//! `cru init` and the setup wizard ask which provider to use. That answer is a
//! fact about the machine — which backend is installed, which key the user
//! pasted — not a preference authored in a config file. It used to be written
//! into `[llm.providers.*]`, inside a file the user hand-edits, where a later
//! edit could drop it silently and where a config language with conditionals
//! makes "the user removed it" indistinguishable from "the branch did not run".
//!
//! Same reasoning as `kilns.json`, same shape, same file family.
//!
//! # The two layers, and which one wins
//!
//! **The config layer wins on a provider-name conflict**, the one precedence
//! rule Crucible applies to every registry
//! ([`crucible_core::config::overlay_layers`]). A provider the config declares
//! is the provider that is used; the state entry stays in this file, shadowed,
//! and comes back from [`LlmStateStore::overlay_onto`] so the daemon can say so
//! at startup. A user cannot act on a conflict they cannot see.
//!
//! `default` follows the same rule: a config that names one keeps it.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use crucible_core::config::{overlay_layers, LlmConfig, LlmProviderConfig};
use serde::{Deserialize, Serialize};

use crate::registry_store::RegistryStore;

/// The schema version this daemon writes and understands.
pub const LLM_STATE_VERSION: u32 = 1;

/// The file name under the daemon data root.
pub const LLM_STATE_FILE: &str = "llm.json";

/// The whole file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmStateFile {
    /// Written on every save so a future reader can refuse a file it is too
    /// old to model, rather than rewriting it through the wrong struct.
    pub version: u32,
    /// The provider to use when nothing else says. `None` means the state
    /// layer expresses no opinion, not "no provider".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Provider instances, by the key they are addressed under.
    ///
    /// `BTreeMap` so the file is byte-stable: a `HashMap` reorders on every
    /// write and turns every save into a diff.
    #[serde(default)]
    pub providers: BTreeMap<String, LlmProviderConfig>,
}

impl Default for LlmStateFile {
    fn default() -> Self {
        Self {
            version: LLM_STATE_VERSION,
            default: None,
            providers: BTreeMap::new(),
        }
    }
}

/// A provider the config layer out-ranked.
#[derive(Debug, Clone)]
pub struct ShadowedProvider {
    /// The contested key.
    pub name: String,
    /// The type the config layer gives it.
    pub config_type: String,
    /// The type the state layer gives it.
    pub state_type: String,
}

/// The reader and the writer of `llm.json`.
pub struct LlmStateStore {
    store: RegistryStore<LlmStateFile>,
}

impl LlmStateStore {
    #[must_use]
    pub fn new(data_home: &Path) -> Self {
        Self {
            store: RegistryStore::new(data_home.join(LLM_STATE_FILE)),
        }
    }

    /// Where the file lives. Named in replies so a user can go and look.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.store.path()
    }

    /// The file as it stands. A missing file is an empty selection, not an
    /// error: no provider has been chosen yet.
    pub fn read(&self) -> Result<LlmStateFile> {
        self.store.read()
    }

    /// Record a provider choice.
    ///
    /// Unlike a kiln name, a provider entry IS re-pointable: choosing Ollama
    /// and later choosing Anthropic is the ordinary thing a user does, and
    /// nothing persisted refers to a provider the way a session refers to a
    /// kiln by name. So this overwrites rather than refusing.
    pub fn register_provider(
        &self,
        name: &str,
        provider_type: crucible_core::config::BackendType,
        model: &str,
        make_default: bool,
    ) -> Result<()> {
        let file = self.path().to_path_buf();
        self.store.update(|state| {
            *state = gate_version(std::mem::take(state), &file)?;
            let entry = state
                .providers
                .entry(name.to_string())
                .or_insert_with(|| LlmProviderConfig::builder(provider_type).build());
            entry.provider_type = provider_type;
            entry.default_model = Some(model.to_string());
            if make_default || state.default.is_none() {
                state.default = Some(name.to_string());
            }
            Ok(())
        })
    }

    /// Merge this file UNDER `config`, and say what the merge out-ranked.
    ///
    /// Mutates rather than returning a new `LlmConfig` because there is exactly
    /// one place the daemon holds its provider table, and handing back a second
    /// copy is how two of them start to exist.
    pub fn overlay_onto(&self, config: &mut LlmConfig) -> Vec<ShadowedProvider> {
        let Ok(state) = self.read() else {
            // An unreadable state file must not take the daemon's provider
            // table with it: the config layer alone is a working config.
            return Vec::new();
        };

        let merged = overlay_layers(
            config
                .providers
                .iter()
                .map(|(name, entry)| (name.clone(), entry.clone())),
            state.providers,
            |(name, _)| name.clone(),
            // Same provider TYPE is the same selection written down twice.
            // Model and key differences are the config refining an entry, not
            // a conflict worth telling the user about.
            |(_, declared), (_, entry)| declared.provider_type == entry.provider_type,
        );

        config.providers = merged.effective.into_iter().collect();
        // The config's own `default` out-ranks the state's, and a `default`
        // naming a provider that does not exist is worse than none: one
        // consumer reads it as a name, another as "no default".
        if config.default.is_none() {
            config.default = state
                .default
                .filter(|name| config.providers.contains_key(name));
        }

        merged
            .shadowed
            .into_iter()
            .map(|s| ShadowedProvider {
                name: s.name,
                config_type: s.config.1.provider_type.as_str().to_string(),
                state_type: s.state.1.provider_type.as_str().to_string(),
            })
            .collect()
    }
}

/// Refuse a file this daemon is too old to read.
///
/// Fail closed: a higher version means keys this build does not model, and
/// writing the file back would erase them.
fn gate_version(state: LlmStateFile, path: &Path) -> Result<LlmStateFile> {
    anyhow::ensure!(
        state.version <= LLM_STATE_VERSION,
        "{} is version {}; this daemon understands version {LLM_STATE_VERSION}. \
         The daemon is older than the file — upgrade Crucible.",
        path.display(),
        state.version
    );
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::BackendType;
    use tempfile::TempDir;

    fn provider(kind: BackendType, model: &str) -> LlmProviderConfig {
        let mut entry = LlmProviderConfig::builder(kind).build();
        entry.default_model = Some(model.to_string());
        entry
    }

    fn config_with(entries: &[(&str, BackendType, &str)], default: Option<&str>) -> LlmConfig {
        let mut config = LlmConfig::default();
        for (name, kind, model) in entries {
            config
                .providers
                .insert((*name).to_string(), provider(*kind, model));
        }
        config.default = default.map(str::to_string);
        config
    }

    #[test]
    fn a_selection_survives_a_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());

        assert!(
            store.read().unwrap().providers.is_empty(),
            "a missing file is an empty selection, not an error"
        );

        store
            .register_provider("anthropic", BackendType::Anthropic, "claude-sonnet", false)
            .unwrap();

        let read = LlmStateStore::new(tmp.path()).read().unwrap();
        assert_eq!(read.version, LLM_STATE_VERSION);
        assert_eq!(read.default.as_deref(), Some("anthropic"));
        assert_eq!(
            read.providers["anthropic"].default_model.as_deref(),
            Some("claude-sonnet")
        );
    }

    /// Unlike a kiln name, a provider IS re-pointable. Choosing Ollama and
    /// later choosing Anthropic is the ordinary thing a user does, and nothing
    /// persisted refers to a provider the way a session refers to a kiln.
    #[test]
    fn choosing_a_different_provider_replaces_the_selection() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());

        store
            .register_provider("ollama", BackendType::Ollama, "llama3.2", true)
            .unwrap();
        store
            .register_provider("anthropic", BackendType::Anthropic, "claude-sonnet", true)
            .unwrap();

        let read = store.read().unwrap();
        assert_eq!(read.default.as_deref(), Some("anthropic"));
        assert_eq!(
            read.providers.len(),
            2,
            "the old entry stays available; only the default moved"
        );
    }

    /// The whole reason the state layer exists: a provider the config never
    /// mentions still reaches the daemon.
    #[test]
    fn a_recorded_provider_the_config_never_mentions_is_used() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());
        store
            .register_provider("ollama", BackendType::Ollama, "llama3.2", true)
            .unwrap();

        let mut config = config_with(&[], None);
        let shadowed = store.overlay_onto(&mut config);

        assert!(shadowed.is_empty());
        assert_eq!(
            config.providers["ollama"].default_model.as_deref(),
            Some("llama3.2")
        );
        assert_eq!(config.default.as_deref(), Some("ollama"));
    }

    /// One precedence rule, applied here too: the config out-ranks the state
    /// layer on a provider key, and the loser is reported rather than dropped.
    #[test]
    fn the_config_wins_a_provider_key_the_state_also_names() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());
        store
            .register_provider("main", BackendType::Ollama, "llama3.2", true)
            .unwrap();

        let mut config = config_with(&[("main", BackendType::Anthropic, "claude-sonnet")], None);
        let shadowed = store.overlay_onto(&mut config);

        assert_eq!(
            config.providers["main"].provider_type,
            BackendType::Anthropic,
            "the config's provider is the one used"
        );
        assert_eq!(shadowed.len(), 1, "the loser must be reported");
        assert_eq!(shadowed[0].name, "main");
        assert_eq!(shadowed[0].state_type, "ollama");
        assert_eq!(shadowed[0].config_type, "anthropic");
    }

    /// The same provider in both layers is one selection written down twice.
    /// There is nothing for the user to resolve, so nothing is reported.
    #[test]
    fn the_same_provider_in_both_layers_is_not_a_conflict() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());
        store
            .register_provider("ollama", BackendType::Ollama, "llama3.2", true)
            .unwrap();

        let mut config = config_with(&[("ollama", BackendType::Ollama, "qwen2.5")], None);
        let shadowed = store.overlay_onto(&mut config);

        assert!(
            shadowed.is_empty(),
            "same provider type in both layers is not a conflict: {shadowed:?}"
        );
        assert_eq!(
            config.providers["ollama"].default_model.as_deref(),
            Some("qwen2.5"),
            "the config's refinement of the entry stands"
        );
    }

    #[test]
    fn the_configs_default_out_ranks_the_recorded_one() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());
        store
            .register_provider("ollama", BackendType::Ollama, "llama3.2", true)
            .unwrap();

        let mut config = config_with(
            &[("anthropic", BackendType::Anthropic, "claude-sonnet")],
            Some("anthropic"),
        );
        store.overlay_onto(&mut config);

        assert_eq!(config.default.as_deref(), Some("anthropic"));
    }

    /// A `default` naming a provider that does not exist is worse than none:
    /// `LlmConfig::default_provider` returns `None` for it, and a caller that
    /// reads the string without looking it up gets a name nothing answers to.
    #[test]
    fn a_recorded_default_naming_no_surviving_provider_is_dropped() {
        let tmp = TempDir::new().unwrap();
        let store = LlmStateStore::new(tmp.path());
        store
            .store
            .update(|state: &mut LlmStateFile| {
                state.default = Some("vanished".to_string());
                Ok(())
            })
            .unwrap();

        let mut config = config_with(&[("ollama", BackendType::Ollama, "llama3.2")], None);
        store.overlay_onto(&mut config);

        assert_eq!(config.default, None, "a dangling default must not survive");
    }

    /// Fail closed on a file from a newer Crucible, and do not rewrite it.
    #[test]
    fn a_newer_file_is_refused_rather_than_rewritten() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(LLM_STATE_FILE);
        std::fs::write(
            &path,
            r#"{"version": 99, "providers": {"future": {"type": "ollama"}}}"#,
        )
        .unwrap();
        let store = LlmStateStore::new(tmp.path());

        let err = store
            .register_provider("ollama", BackendType::Ollama, "llama3.2", true)
            .expect_err("a newer file must not be written through this struct");
        assert!(err.to_string().contains("llm.json"), "{err}");
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("\"version\": 99"),
            "the file must be left exactly as it was"
        );
    }
}
