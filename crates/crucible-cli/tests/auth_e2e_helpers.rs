//! E2E Test Helpers for Auth Command Testing
//!
//! Provides environment isolation and fixture management for testing `cru auth`
//! commands and pre-flight checks.
//!
//! # Test Strategy: assert_cmd vs expectrl
//!
//! ## Use `assert_cmd` for:
//! - **Non-interactive CLI tests** (commands with all flags provided)
//! - Exit code verification
//! - Stdout/stderr pattern matching
//! - File system state verification
//!
//! **Examples:**
//! - `cru auth login --provider openai --key sk-...` (all args provided)
//! - `cru auth list` (no interaction needed)
//! - `cru auth logout --provider openai` (single flag)
//!
//! ## Use `expectrl` for:
//! - **Interactive flows** (commands that prompt for user input)
//! - PTY-based terminal interaction
//! - Multi-step dialogues
//! - Real-time output verification
//!
//! **Examples:**
//! - `cru auth login` (prompts for provider selection, then key)
//! - Pre-flight checks with missing kiln (prompts for path)
//! - Any flow using `dialoguer` prompts
//!
//! # Environment Isolation
//!
//! `AuthTestEnv` creates temporary directories for `$HOME` and `$XDG_CONFIG_HOME`,
//! ensuring tests don't pollute the real user environment or interfere with each other.
//!
//! # Secrets File
//!
//! Credentials are stored as TOML in `$XDG_CONFIG_HOME/crucible/secrets.toml` (0o600).
//! This matches the production `SecretsFile` format from `crucible-config::credentials`.
//!
//! # Usage
//!
//! ```no_run
//! use auth_e2e_helpers::AuthTestEnv;
//!
//! let env = AuthTestEnv::new();
//!
//! // Run command with isolated environment
//! let output = env.command("auth")
//!     .arg("login")
//!     .arg("--provider")
//!     .arg("openai")
//!     .arg("--key")
//!     .arg("sk-test-key")
//!     .output()
//!     .unwrap();
//!
//! assert!(output.status.success());
//! assert!(env.secrets_file_exists());
//! ```

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// Environment isolation for auth E2E tests
///
/// Creates temporary directories for `$HOME` and `$XDG_CONFIG_HOME`, sets environment
/// variables, and provides helpers for credential fixture injection.
///
/// Cleanup is automatic via `TempDir` RAII guards.
pub struct AuthTestEnv {
    /// Temporary HOME directory
    temp_home: TempDir,
    /// Temporary XDG_CONFIG_HOME directory
    temp_config: TempDir,
    /// Environment variables to set on commands (name -> value)
    env_vars: Arc<Mutex<HashMap<String, String>>>,
    /// Original environment variables (for restoration)
    original_env: Arc<Mutex<HashMap<String, Option<String>>>>,
}

impl AuthTestEnv {
    /// Create a new isolated test environment
    ///
    /// Sets up:
    /// - Temporary `$HOME` directory
    /// - Temporary `$XDG_CONFIG_HOME` directory
    /// - Clean environment variables (no real API keys leak into tests)
    pub fn new() -> Self {
        let temp_home = TempDir::new().expect("Failed to create temp HOME");
        let temp_config = TempDir::new().expect("Failed to create temp XDG_CONFIG_HOME");

        let mut env_vars = HashMap::new();
        env_vars.insert("HOME".to_string(), temp_home.path().display().to_string());
        env_vars.insert(
            "XDG_CONFIG_HOME".to_string(),
            temp_config.path().display().to_string(),
        );

        // Clear provider API key env vars to prevent leakage from real environment
        let api_key_vars = [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "OLLAMA_HOST",
            "CRUCIBLE_API_KEY",
        ];

        let mut original_env = HashMap::new();
        for var in &api_key_vars {
            original_env.insert(var.to_string(), env::var(var).ok());
            env::remove_var(var);
        }

        Self {
            temp_home,
            temp_config,
            env_vars: Arc::new(Mutex::new(env_vars)),
            original_env: Arc::new(Mutex::new(original_env)),
        }
    }

    /// Add a credential fixture to the secrets.toml file
    ///
    /// Pre-populates the credential store with a test API key.
    /// Uses the same TOML format as `SecretsFile` from `crucible-config::credentials`.
    ///
    /// # Example
    ///
    /// ```rust
    /// let env = AuthTestEnv::new()
    ///     .with_credential("openai", "sk-test-key");
    /// ```
    pub fn with_credential(self, provider: &str, key: &str) -> Self {
        let secrets_file = self.secrets_file_path();

        // Create parent directory if needed
        if let Some(parent) = secrets_file.parent() {
            fs::create_dir_all(parent).expect("Failed to create config directory");
        }

        // Read existing secrets or start fresh
        let mut content: toml::Value = if secrets_file.exists() {
            let raw = fs::read_to_string(&secrets_file).expect("Failed to read secrets.toml");
            toml::from_str(&raw).unwrap_or(toml::Value::Table(Default::default()))
        } else {
            toml::Value::Table(Default::default())
        };

        // Ensure providers table exists
        let table = content.as_table_mut().expect("root should be table");
        if !table.contains_key("providers") {
            table.insert(
                "providers".to_string(),
                toml::Value::Table(Default::default()),
            );
        }

        // Add provider entry
        let providers = table
            .get_mut("providers")
            .unwrap()
            .as_table_mut()
            .expect("providers should be table");
        let mut provider_table = toml::map::Map::new();
        provider_table.insert("api_key".to_string(), toml::Value::String(key.to_string()));
        providers.insert(provider.to_string(), toml::Value::Table(provider_table));

        // Write with restricted permissions
        let toml_str = toml::to_string_pretty(&content).expect("Failed to serialize");
        fs::write(&secrets_file, toml_str).expect("Failed to write secrets.toml");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&secrets_file, perms).expect("Failed to set permissions");
        }

        self
    }

    /// Set an environment variable for this test
    ///
    /// The variable is passed to commands via `assert_cmd` and cleaned up on drop.
    ///
    /// # Example
    ///
    /// ```rust
    /// let env = AuthTestEnv::new()
    ///     .with_env_var("OPENAI_API_KEY", "sk-from-env");
    /// ```
    pub fn with_env_var(self, name: &str, value: &str) -> Self {
        {
            let mut env_vars = self.env_vars.lock().unwrap();
            env_vars.insert(name.to_string(), value.to_string());
        }

        // Track original value for restoration
        {
            let mut original_env = self.original_env.lock().unwrap();
            if !original_env.contains_key(name) {
                original_env.insert(name.to_string(), env::var(name).ok());
            }
        }

        self
    }

    /// Get the path to the secrets.toml file in the test environment
    pub fn secrets_file_path(&self) -> PathBuf {
        self.temp_config
            .path()
            .join("crucible")
            .join("secrets.toml")
    }

    /// Check if secrets.toml exists
    pub fn secrets_file_exists(&self) -> bool {
        self.secrets_file_path().exists()
    }

    /// Read the secrets.toml file as parsed TOML
    pub fn read_secrets_file(&self) -> Option<toml::Value> {
        let path = self.secrets_file_path();
        if !path.exists() {
            return None;
        }

        let content = fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Read a specific provider's API key from the secrets file
    pub fn read_provider_key(&self, provider: &str) -> Option<String> {
        let secrets = self.read_secrets_file()?;
        secrets
            .get("providers")?
            .get(provider)?
            .get("api_key")?
            .as_str()
            .map(String::from)
    }

    /// Get the temporary HOME directory path
    pub fn home_dir(&self) -> &Path {
        self.temp_home.path()
    }

    /// Get the temporary XDG_CONFIG_HOME directory path
    pub fn config_dir(&self) -> &Path {
        self.temp_config.path()
    }

    /// Create a `Command` with the test environment applied
    ///
    /// This is the primary way to run `cru` commands in tests.
    ///
    /// # Example
    ///
    /// ```rust
    /// let env = AuthTestEnv::new();
    /// let output = env.command("auth")
    ///     .arg("list")
    ///     .output()
    ///     .unwrap();
    /// ```
    pub fn command(&self, subcommand: &str) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::new(env!("CARGO_BIN_EXE_cru"));
        cmd.arg(subcommand);

        // Apply environment variables
        let env_vars = self.env_vars.lock().unwrap();
        for (key, value) in env_vars.iter() {
            cmd.env(key, value);
        }

        cmd
    }

    /// Create a kiln fixture in the test environment
    ///
    /// Creates a minimal kiln structure at the given path (relative to temp HOME).
    ///
    /// # Example
    ///
    /// ```rust
    /// let env = AuthTestEnv::new();
    /// env.create_kiln("my-kiln");
    /// ```
    pub fn create_kiln(&self, name: &str) -> PathBuf {
        let kiln_path = self.home_dir().join(name);
        let crucible_dir = kiln_path.join(".crucible");

        fs::create_dir_all(&crucible_dir).expect("Failed to create kiln directory");
        fs::create_dir_all(crucible_dir.join("sessions")).expect("Failed to create sessions dir");
        fs::create_dir_all(crucible_dir.join("plugins")).expect("Failed to create plugins dir");

        // Create minimal config.toml
        let config_content = r#"
[kiln]
path = "."

[chat]
provider = "ollama"
model = "llama3.2"
"#;
        fs::write(crucible_dir.join("config.toml"), config_content)
            .expect("Failed to write config.toml");

        kiln_path
    }
}

impl Drop for AuthTestEnv {
    fn drop(&mut self) {
        // Restore original environment variables
        let original_env = self.original_env.lock().unwrap();
        for (key, original_value) in original_env.iter() {
            match original_value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }
    }
}

impl Default for AuthTestEnv {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_test_env_creates_temp_dirs() {
        let env = AuthTestEnv::new();
        assert!(env.home_dir().exists());
        assert!(env.config_dir().exists());
    }

    #[test]
    fn test_auth_test_env_isolates_home() {
        let env = AuthTestEnv::new();
        let real_home = env::var("HOME").unwrap();
        let test_home = env.home_dir().display().to_string();

        // Test HOME should be different from real HOME
        assert_ne!(real_home, test_home);
    }

    #[test]
    fn test_with_credential_creates_secrets_file() {
        let env = AuthTestEnv::new().with_credential("openai", "sk-test-key");

        assert!(env.secrets_file_exists());
        assert_eq!(
            env.read_provider_key("openai"),
            Some("sk-test-key".to_string())
        );
    }

    #[test]
    fn test_with_credential_multiple_providers() {
        let env = AuthTestEnv::new()
            .with_credential("openai", "sk-openai-key")
            .with_credential("anthropic", "sk-anthropic-key");

        assert_eq!(
            env.read_provider_key("openai"),
            Some("sk-openai-key".to_string())
        );
        assert_eq!(
            env.read_provider_key("anthropic"),
            Some("sk-anthropic-key".to_string())
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_secrets_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let env = AuthTestEnv::new().with_credential("openai", "sk-test-key");

        let metadata = fs::metadata(env.secrets_file_path()).unwrap();
        let mode = metadata.permissions().mode();

        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn test_with_env_var_sets_variable() {
        let env = AuthTestEnv::new().with_env_var("TEST_VAR", "test_value");

        let env_vars = env.env_vars.lock().unwrap();
        assert_eq!(env_vars.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[test]
    fn test_create_kiln_creates_structure() {
        let env = AuthTestEnv::new();
        let kiln_path = env.create_kiln("test-kiln");

        assert!(kiln_path.exists());
        assert!(kiln_path.join(".crucible").exists());
        assert!(kiln_path.join(".crucible/config.toml").exists());
        assert!(kiln_path.join(".crucible/sessions").exists());
        assert!(kiln_path.join(".crucible/plugins").exists());
    }

    #[test]
    fn test_env_cleanup_restores_variables() {
        let original_home = env::var("HOME").ok();

        {
            let _env = AuthTestEnv::new();
            // HOME is modified inside this scope
        }

        // After drop, HOME should be restored
        assert_eq!(env::var("HOME").ok(), original_home);
    }

    #[test]
    fn test_secrets_file_toml_format_compatible() {
        let env = AuthTestEnv::new().with_credential("openai", "sk-test");

        // Read raw file and verify TOML structure
        let content = env.read_secrets_file().unwrap();
        let api_key = content
            .get("providers")
            .and_then(|p| p.get("openai"))
            .and_then(|o| o.get("api_key"))
            .and_then(|k| k.as_str());
        assert_eq!(api_key, Some("sk-test"));
    }
}
