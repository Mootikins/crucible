//! Credential storage and resolution for LLM provider API keys
//!
//! Stores secrets in a TOML file (`~/.config/crucible/secrets.toml`) with `0o600`
//! permissions. The file is designed to be included into the main config via the
//! existing `[include]` mechanism:
//!
//! ```toml
//! # config.toml
//! [include]
//! llm = "secrets.toml"
//! ```
//!
//! # Secrets File Format
//!
//! ```toml
//! # ~/.config/crucible/secrets.toml (0o600)
//! # Managed by `cru auth`. Do not edit while crucible is running.
//!
//! [providers.openai]
//! api_key = "sk-..."
//!
//! [providers.anthropic]
//! api_key = "sk-ant-..."
//! ```
//!
//! When included as `[include] llm = "secrets.toml"`, the `providers.*` sections
//! deep-merge into `LlmConfig.providers`, adding `api_key` values to existing
//! provider configurations.
//!
//! # Resolution Priority
//!
//! [`resolve_api_key`] checks sources in this order:
//! 1. Environment variable (e.g., `OPENAI_API_KEY`)
//! 2. Credential store (secrets file or keyring)
//! 3. Config file value (already resolved from `{env:VAR}` / `{file:path}`)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// A provider's stored secrets
///
/// Matches the shape of `LlmProviderConfig` fields so it can deep-merge
/// via the include system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSecrets {
    /// API key for the provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    // Future: oauth_token, refresh_token, expires_at for OAuth providers
}

/// Top-level structure of secrets.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SecretsFileContent {
    /// Provider secrets keyed by provider name
    #[serde(default)]
    pub providers: HashMap<String, ProviderSecrets>,
}

/// Errors from credential store operations
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// IO error reading/writing credential store
    #[error("credential store IO error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML serialization error
    #[error("credential store serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),

    /// TOML parse error
    #[error("credential store parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

/// Result type for credential operations
pub type CredentialResult<T> = Result<T, CredentialError>;

/// Trait for credential storage backends
///
/// Abstracts over file-based and keyring-based storage.
pub trait CredentialStore {
    /// Get the API key for a provider
    fn get(&self, provider: &str) -> CredentialResult<Option<String>>;

    /// Store an API key for a provider
    fn set(&mut self, provider: &str, api_key: &str) -> CredentialResult<()>;

    /// Remove credentials for a provider. Returns true if the provider existed.
    fn remove(&mut self, provider: &str) -> CredentialResult<bool>;

    /// List all stored provider → API key pairs
    fn list(&self) -> CredentialResult<HashMap<String, String>>;
}

/// TOML-based credential store
///
/// Reads/writes `secrets.toml` in the crucible config directory.
/// File permissions are set to `0o600` (owner read/write only).
///
/// The file format matches the `LlmConfig` structure so it can be
/// deep-merged via `[include] llm = "secrets.toml"`.
pub struct SecretsFile {
    path: PathBuf,
}

impl SecretsFile {
    /// Create a SecretsFile at the default path
    ///
    /// Default: `~/.config/crucible/secrets.toml`
    pub fn new() -> Self {
        Self {
            path: Self::default_path(),
        }
    }

    /// Create a SecretsFile with a custom path (primarily for testing)
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Default secrets file path: `$XDG_CONFIG_HOME/crucible/secrets.toml`
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("crucible")
            .join("secrets.toml")
    }

    /// Get the path to the secrets file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the secrets file from disk
    fn read(&self) -> CredentialResult<SecretsFileContent> {
        if !self.path.exists() {
            return Ok(SecretsFileContent::default());
        }

        let content = std::fs::read_to_string(&self.path)?;
        if content.trim().is_empty() {
            return Ok(SecretsFileContent::default());
        }

        match toml::from_str(&content) {
            Ok(parsed) => Ok(parsed),
            Err(e) => {
                warn!(
                    "Failed to parse secrets file at {}: {}. Treating as empty.",
                    self.path.display(),
                    e
                );
                Ok(SecretsFileContent::default())
            }
        }
    }

    /// Write the secrets file to disk with restricted permissions
    fn write(&self, content: &SecretsFileContent) -> CredentialResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let toml_str = toml::to_string_pretty(content)?;
        std::fs::write(&self.path, toml_str)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.path, perms)?;
        }

        Ok(())
    }
}

impl Default for SecretsFile {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for SecretsFile {
    fn get(&self, provider: &str) -> CredentialResult<Option<String>> {
        let content = self.read()?;
        Ok(content
            .providers
            .get(provider)
            .and_then(|s| s.api_key.clone()))
    }

    fn set(&mut self, provider: &str, api_key: &str) -> CredentialResult<()> {
        let mut content = self.read()?;
        content.providers.insert(
            provider.to_string(),
            ProviderSecrets {
                api_key: Some(api_key.to_string()),
            },
        );
        self.write(&content)
    }

    fn remove(&mut self, provider: &str) -> CredentialResult<bool> {
        let mut content = self.read()?;
        let existed = content.providers.remove(provider).is_some();
        if existed {
            self.write(&content)?;
        }
        Ok(existed)
    }

    fn list(&self) -> CredentialResult<HashMap<String, String>> {
        let content = self.read()?;
        Ok(content
            .providers
            .into_iter()
            .filter_map(|(name, secrets)| secrets.api_key.map(|key| (name, key)))
            .collect())
    }
}

/// Keyring-backed credential store (OS-native secret storage)
///
/// Uses the system keyring (macOS Keychain, Windows Credential Vault,
/// Linux Secret Service / libsecret) via the `keyring` crate.
///
/// Each provider is stored as a separate entry with service name `crucible`.
#[cfg(feature = "keyring")]
pub struct KeyringStore {
    service: String,
}

#[cfg(feature = "keyring")]
impl KeyringStore {
    pub fn new() -> Self {
        Self {
            service: "crucible".to_string(),
        }
    }

    fn entry(&self, provider: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(&self.service, provider)
            .map_err(|e| CredentialError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
    }
}

#[cfg(feature = "keyring")]
impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "keyring")]
impl CredentialStore for KeyringStore {
    fn get(&self, provider: &str) -> CredentialResult<Option<String>> {
        let entry = self.entry(provider)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => {
                warn!("Keyring error reading {}: {}", provider, e);
                Ok(None)
            }
        }
    }

    fn set(&mut self, provider: &str, api_key: &str) -> CredentialResult<()> {
        let entry = self.entry(provider)?;
        entry
            .set_password(api_key)
            .map_err(|e| CredentialError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
    }

    fn remove(&mut self, provider: &str) -> CredentialResult<bool> {
        let entry = self.entry(provider)?;
        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(CredentialError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e,
            ))),
        }
    }

    fn list(&self) -> CredentialResult<HashMap<String, String>> {
        let mut result = HashMap::new();
        let known = ["openai", "anthropic", "ollama"];
        for provider in &known {
            if let Ok(Some(key)) = self.get(provider) {
                result.insert(provider.to_string(), key);
            }
        }
        Ok(result)
    }
}

/// Auto-selecting credential store that tries keyring first, falls back to file
///
/// When the `keyring` feature is enabled, attempts to use the OS keyring.
/// If keyring operations fail, transparently falls back to the TOML file store.
/// Without the `keyring` feature, this is equivalent to `SecretsFile`.
pub struct AutoStore {
    file: SecretsFile,
    #[cfg(feature = "keyring")]
    keyring: KeyringStore,
}

impl AutoStore {
    pub fn new() -> Self {
        Self {
            file: SecretsFile::new(),
            #[cfg(feature = "keyring")]
            keyring: KeyringStore::new(),
        }
    }

    pub fn with_file_path(path: PathBuf) -> Self {
        Self {
            file: SecretsFile::with_path(path),
            #[cfg(feature = "keyring")]
            keyring: KeyringStore::new(),
        }
    }
}

impl Default for AutoStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for AutoStore {
    fn get(&self, provider: &str) -> CredentialResult<Option<String>> {
        #[cfg(feature = "keyring")]
        {
            match self.keyring.get(provider) {
                Ok(Some(key)) => return Ok(Some(key)),
                Ok(None) => {}
                Err(e) => {
                    debug!(
                        "Keyring get failed for {}, falling back to file: {}",
                        provider, e
                    );
                }
            }
        }
        self.file.get(provider)
    }

    fn set(&mut self, provider: &str, api_key: &str) -> CredentialResult<()> {
        #[cfg(feature = "keyring")]
        {
            match self.keyring.set(provider, api_key) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    debug!(
                        "Keyring set failed for {}, falling back to file: {}",
                        provider, e
                    );
                }
            }
        }
        self.file.set(provider, api_key)
    }

    fn remove(&mut self, provider: &str) -> CredentialResult<bool> {
        #[cfg(feature = "keyring")]
        {
            match self.keyring.remove(provider) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(e) => {
                    debug!(
                        "Keyring remove failed for {}, falling back to file: {}",
                        provider, e
                    );
                }
            }
        }
        self.file.remove(provider)
    }

    fn list(&self) -> CredentialResult<HashMap<String, String>> {
        #[cfg(feature = "keyring")]
        {
            let mut combined = match self.keyring.list() {
                Ok(entries) => entries,
                Err(e) => {
                    debug!("Keyring list failed, using file only: {}", e);
                    HashMap::new()
                }
            };
            if let Ok(file_entries) = self.file.list() {
                for (k, v) in file_entries {
                    combined.entry(k).or_insert(v);
                }
            }
            return Ok(combined);
        }
        #[cfg(not(feature = "keyring"))]
        self.file.list()
    }
}

/// Known provider environment variable mappings
pub fn env_var_for_provider(provider: &str) -> Option<&'static str> {
    match provider.to_lowercase().as_str() {
        "openai" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        _ => None,
    }
}

/// Source of a resolved credential
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// From an environment variable
    EnvVar,
    /// From the credential store (secrets file or keyring)
    Store,
    /// From the config file (inline or `{env:VAR}` / `{file:path}`)
    Config,
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialSource::EnvVar => write!(f, "env"),
            CredentialSource::Store => write!(f, "file"),
            CredentialSource::Config => write!(f, "config"),
        }
    }
}

/// Resolve an API key for a provider using the priority chain:
///
/// 1. Environment variable (e.g., `OPENAI_API_KEY`)
/// 2. Credential store (secrets file or keyring)
/// 3. Config value (passed through from `LlmProviderConfig::api_key`)
///
/// Returns `(key, source)` tuple, or `None` if no key is found.
pub fn resolve_api_key(
    provider: &str,
    store: &dyn CredentialStore,
    config_key: Option<&str>,
) -> Option<(String, CredentialSource)> {
    // 1. Environment variable
    if let Some(env_var) = env_var_for_provider(provider) {
        if let Ok(value) = std::env::var(env_var) {
            if !value.is_empty() {
                debug!("Resolved API key for {} from env var {}", provider, env_var);
                return Some((value, CredentialSource::EnvVar));
            }
        }
    }

    // 2. Credential store
    match store.get(provider) {
        Ok(Some(key)) => {
            debug!("Resolved API key for {} from credential store", provider);
            return Some((key, CredentialSource::Store));
        }
        Ok(None) => {}
        Err(e) => {
            warn!("Failed to read credential store for {}: {}", provider, e);
        }
    }

    // 3. Config value
    if let Some(key) = config_key {
        if !key.is_empty() {
            debug!("Resolved API key for {} from config", provider);
            return Some((key.to_string(), CredentialSource::Config));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create a SecretsFile in a temp directory
    fn temp_store() -> (SecretsFile, TempDir) {
        let dir = TempDir::new().expect("create temp dir");
        let path = dir.path().join("secrets.toml");
        let store = SecretsFile::with_path(path);
        (store, dir)
    }

    // =========================================================================
    // SecretsFile: set/get/remove roundtrip
    // =========================================================================

    #[test]
    fn secrets_file_set_get_remove_roundtrip() {
        let (mut store, _dir) = temp_store();

        store.set("openai", "sk-test-key").expect("set");

        let key = store.get("openai").expect("get");
        assert_eq!(key, Some("sk-test-key".to_string()));

        let removed = store.remove("openai").expect("remove");
        assert!(removed);

        let key = store.get("openai").expect("get after remove");
        assert_eq!(key, None);
    }

    #[test]
    fn secrets_file_remove_nonexistent_returns_false() {
        let (mut store, _dir) = temp_store();
        let removed = store.remove("nonexistent").expect("remove");
        assert!(!removed);
    }

    // =========================================================================
    // SecretsFile: file permissions
    // =========================================================================

    #[test]
    #[cfg(unix)]
    fn secrets_file_creates_with_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (mut store, _dir) = temp_store();
        store.set("openai", "sk-test").expect("set");

        let metadata = std::fs::metadata(store.path()).expect("metadata");
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "File should be owner-only rw");
    }

    // =========================================================================
    // SecretsFile: missing file
    // =========================================================================

    #[test]
    fn secrets_file_handles_missing_file() {
        let (store, _dir) = temp_store();

        let key = store.get("openai").expect("get");
        assert_eq!(key, None);

        let list = store.list().expect("list");
        assert!(list.is_empty());
    }

    // =========================================================================
    // SecretsFile: corrupted file
    // =========================================================================

    #[test]
    fn secrets_file_handles_corrupted_toml() {
        let (store, _dir) = temp_store();

        if let Some(parent) = store.path().parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(store.path(), "not valid toml {{{").unwrap();

        // Should return empty, not error
        let key = store.get("openai").expect("get");
        assert_eq!(key, None);

        let list = store.list().expect("list");
        assert!(list.is_empty());
    }

    // =========================================================================
    // SecretsFile: multiple providers
    // =========================================================================

    #[test]
    fn secrets_file_multiple_providers() {
        let (mut store, _dir) = temp_store();

        store.set("openai", "sk-openai").expect("set openai");
        store
            .set("anthropic", "sk-anthropic")
            .expect("set anthropic");

        let list = store.list().expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list["openai"], "sk-openai");
        assert_eq!(list["anthropic"], "sk-anthropic");
    }

    #[test]
    fn secrets_file_overwrite_existing() {
        let (mut store, _dir) = temp_store();

        store.set("openai", "old-key").expect("set");
        store.set("openai", "new-key").expect("overwrite");

        let key = store.get("openai").expect("get");
        assert_eq!(key, Some("new-key".to_string()));
    }

    // =========================================================================
    // SecretsFile: TOML format matches config structure
    // =========================================================================

    #[test]
    fn secrets_file_toml_format_is_config_compatible() {
        let (mut store, _dir) = temp_store();

        store.set("openai", "sk-test").expect("set");

        let raw = std::fs::read_to_string(store.path()).expect("read");

        // Should parse as valid TOML with [providers.openai] section
        let parsed: toml::Value = toml::from_str(&raw).expect("parse");
        let api_key = parsed
            .get("providers")
            .and_then(|p| p.get("openai"))
            .and_then(|o| o.get("api_key"))
            .and_then(|k| k.as_str());
        assert_eq!(api_key, Some("sk-test"));
    }

    // =========================================================================
    // resolve_api_key: env var wins
    // =========================================================================

    #[test]
    fn resolve_api_key_env_var_wins() {
        let (mut store, _dir) = temp_store();
        store.set("openai", "store-key").expect("set");

        std::env::set_var("OPENAI_API_KEY", "env-key");

        let result = resolve_api_key("openai", &store, Some("config-key"));
        assert_eq!(
            result,
            Some(("env-key".to_string(), CredentialSource::EnvVar))
        );

        std::env::remove_var("OPENAI_API_KEY");
    }

    // =========================================================================
    // resolve_api_key: falls back to store
    // =========================================================================

    #[test]
    fn resolve_api_key_falls_back_to_store() {
        let (mut store, _dir) = temp_store();
        store.set("openai", "store-key").expect("set");

        std::env::remove_var("OPENAI_API_KEY");

        let result = resolve_api_key("openai", &store, Some("config-key"));
        assert_eq!(
            result,
            Some(("store-key".to_string(), CredentialSource::Store))
        );
    }

    // =========================================================================
    // resolve_api_key: falls back to config
    // =========================================================================

    #[test]
    fn resolve_api_key_falls_back_to_config() {
        let (store, _dir) = temp_store();
        std::env::remove_var("OPENAI_API_KEY");

        let result = resolve_api_key("openai", &store, Some("config-key"));
        assert_eq!(
            result,
            Some(("config-key".to_string(), CredentialSource::Config))
        );
    }

    // =========================================================================
    // resolve_api_key: returns None when nothing configured
    // =========================================================================

    #[test]
    fn resolve_api_key_returns_none_when_nothing_configured() {
        let (store, _dir) = temp_store();
        std::env::remove_var("OPENAI_API_KEY");

        let result = resolve_api_key("openai", &store, None);
        assert_eq!(result, None);
    }

    // =========================================================================
    // resolve_api_key: unknown provider (no env var mapping)
    // =========================================================================

    #[test]
    fn resolve_api_key_unknown_provider_uses_store() {
        let (mut store, _dir) = temp_store();
        store.set("custom-provider", "custom-key").expect("set");

        let result = resolve_api_key("custom-provider", &store, None);
        assert_eq!(
            result,
            Some(("custom-key".to_string(), CredentialSource::Store))
        );
    }

    // =========================================================================
    // CredentialSource display
    // =========================================================================

    #[test]
    fn credential_source_display() {
        assert_eq!(CredentialSource::EnvVar.to_string(), "env");
        assert_eq!(CredentialSource::Store.to_string(), "file");
        assert_eq!(CredentialSource::Config.to_string(), "config");
    }

    // =========================================================================
    // env_var_for_provider
    // =========================================================================

    #[test]
    fn env_var_mappings() {
        assert_eq!(env_var_for_provider("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(env_var_for_provider("OpenAI"), Some("OPENAI_API_KEY"));
        assert_eq!(env_var_for_provider("anthropic"), Some("ANTHROPIC_API_KEY"));
        assert_eq!(env_var_for_provider("ollama"), None);
        assert_eq!(env_var_for_provider("unknown"), None);
    }
}
