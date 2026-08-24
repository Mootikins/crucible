//! Pattern storage for whitelisting commands, paths, and tools
//!
//! Provides persistent storage for allow-patterns in `~/.config/crucible/whitelists.d/`.
//! Patterns are stored per-project using a hash of the project path.
//!
//! # Example
//!
//! ```rust,no_run
//! use crucible_core::config::PatternStore;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Load patterns for a project
//! let mut store = PatternStore::load("/path/to/project").await?;
//!
//! // Add patterns
//! store.add_bash_pattern("cargo build")?;
//! store.add_file_pattern("src/")?;
//! store.add_tool_pattern("read_note")?;
//!
//! // Save patterns
//! store.save("/path/to/project").await?;
//! # Ok(())
//! # }
//! ```

use super::components::permissions::split_command_line;
use crate::interaction::PermissionScope;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during pattern operations
#[derive(Debug, Error)]
pub enum PatternError {
    /// Pattern is too permissive (e.g., starts with "*")
    #[error("pattern '{0}' is too permissive - patterns starting with '*' are not allowed")]
    TooPermissive(String),

    /// I/O error during file operations
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parsing error
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// TOML serialization error
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

/// Result type for pattern operations
pub type PatternResult<T> = Result<T, PatternError>;

/// Patterns for bash command whitelisting
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BashPatterns {
    /// Command prefixes that are allowed (e.g., "npm install", "cargo build")
    pub allowed_prefixes: Vec<String>,
}

/// Patterns for file path whitelisting
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FilePatterns {
    /// Path prefixes that are allowed (e.g., "src/", "tests/")
    pub allowed_prefixes: Vec<String>,
}

/// Patterns for tool whitelisting
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ToolPatterns {
    /// Tools that are always allowed (e.g., "read_note", "grep_notes")
    pub always_allow: Vec<String>,
}

/// Storage for allow-patterns organized by category
///
/// Patterns are stored in TOML format in `~/.config/crucible/whitelists.d/<project-hash>.toml`.
/// Each project gets its own pattern file based on a hash of the project path.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PatternStore {
    /// Bash command patterns
    pub bash_commands: BashPatterns,

    /// File path patterns
    pub file_paths: FilePatterns,

    /// Tool patterns
    pub tools: ToolPatterns,
}

impl PatternStore {
    /// Create a new empty pattern store
    pub fn new() -> Self {
        Self::default()
    }

    /// Load patterns for a project from disk
    ///
    /// If no pattern file exists for the project, returns an empty store.
    ///
    /// # Arguments
    ///
    /// * `project_path` - Path to the project directory (used to generate hash)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use crucible_core::config::PatternStore;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let store = PatternStore::load("/home/user/my-project").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn load(project_path: &str) -> PatternResult<Self> {
        let file_path = Self::pattern_file_path(project_path);

        if !file_path.exists() {
            return Ok(Self::new());
        }

        let content = tokio::fs::read_to_string(&file_path).await?;
        let store: PatternStore = toml::from_str(&content)?;
        Ok(store)
    }

    /// Load patterns synchronously (for non-async contexts)
    pub fn load_sync(project_path: &str) -> PatternResult<Self> {
        Self::load_file(&Self::pattern_file_path(project_path))
    }

    /// Load the user-wide store under `dir`, the one a `User` scope grant
    /// writes to. The directory is injected, never read from the environment.
    pub fn load_user_sync_in(dir: &Path) -> PatternResult<Self> {
        Self::load_file(&Self::user_file_in(dir))
    }

    /// [`Self::load_sync`] with the directory injected.
    pub fn load_sync_in(dir: &Path, project_path: &str) -> PatternResult<Self> {
        Self::load_file(&Self::project_file_in(dir, project_path))
    }

    /// Load one store file. A missing file is an empty store.
    pub fn load_file(file_path: &Path) -> PatternResult<Self> {
        if !file_path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(file_path)?;
        let store: PatternStore = toml::from_str(&content)?;
        Ok(store)
    }

    /// Save patterns for a project to disk
    ///
    /// Creates the `whitelists.d/` directory if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `project_path` - Path to the project directory (used to generate hash)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use crucible_core::config::PatternStore;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut store = PatternStore::new();
    /// store.add_bash_pattern("cargo test")?;
    /// store.save("/home/user/my-project").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn save(&self, project_path: &str) -> PatternResult<()> {
        let file_path = Self::pattern_file_path(project_path);
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.save_file(&file_path))
            .await
            .map_err(std::io::Error::other)?
    }

    /// Save patterns synchronously (for non-async contexts)
    pub fn save_sync(&self, project_path: &str) -> PatternResult<()> {
        self.save_file(&Self::pattern_file_path(project_path))
    }

    /// Write this store to one file. Creates the parent directory.
    ///
    /// The store holds permission whitelists, so the file is written with
    /// mode `0o600` through [`crate::fs::write_private`].
    pub fn save_file(&self, file_path: &Path) -> PatternResult<()> {
        let content = toml::to_string_pretty(self)?;
        crate::fs::write_private(file_path, content.as_bytes())?;
        Ok(())
    }

    /// Add a bash command pattern
    ///
    /// # Errors
    ///
    /// Returns `PatternError::TooPermissive` if the pattern starts with "*".
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut store = PatternStore::new();
    /// store.add_bash_pattern("npm install").unwrap();
    /// store.add_bash_pattern("cargo build").unwrap();
    ///
    /// // This will fail - too permissive
    /// assert!(store.add_bash_pattern("*").is_err());
    /// ```
    pub fn add_bash_pattern(&mut self, prefix: &str) -> PatternResult<()> {
        Self::validate_pattern(prefix)?;

        let prefix = prefix.to_string();
        if !self.bash_commands.allowed_prefixes.contains(&prefix) {
            self.bash_commands.allowed_prefixes.push(prefix);
        }
        Ok(())
    }

    /// Add a file path pattern
    ///
    /// # Errors
    ///
    /// Returns `PatternError::TooPermissive` if the pattern starts with "*".
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut store = PatternStore::new();
    /// store.add_file_pattern("src/").unwrap();
    /// store.add_file_pattern("tests/").unwrap();
    ///
    /// // This will fail - too permissive
    /// assert!(store.add_file_pattern("*").is_err());
    /// ```
    pub fn add_file_pattern(&mut self, prefix: &str) -> PatternResult<()> {
        Self::validate_pattern(prefix)?;

        let prefix = prefix.to_string();
        if !self.file_paths.allowed_prefixes.contains(&prefix) {
            self.file_paths.allowed_prefixes.push(prefix);
        }
        Ok(())
    }

    /// Add a tool pattern
    ///
    /// # Errors
    ///
    /// Returns `PatternError::TooPermissive` if the pattern starts with "*".
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut store = PatternStore::new();
    /// store.add_tool_pattern("read_note").unwrap();
    /// store.add_tool_pattern("grep_notes").unwrap();
    ///
    /// // This will fail - too permissive
    /// assert!(store.add_tool_pattern("*").is_err());
    /// ```
    pub fn add_tool_pattern(&mut self, tool_name: &str) -> PatternResult<()> {
        Self::validate_pattern(tool_name)?;

        let tool_name = tool_name.to_string();
        if !self.tools.always_allow.contains(&tool_name) {
            self.tools.always_allow.push(tool_name);
        }
        Ok(())
    }

    /// Check if every statement of a bash command line matches an allowed pattern
    ///
    /// The line is split on `;`, `&&`, `||`, `|`, `&` and newlines. A saved prefix allows
    /// one statement, so a line that chains a second statement after an allowed one only
    /// passes when that statement also matches. A substitution (`$(...)`, backticks,
    /// `<(...)`) hides a command from the splitter, so the line does not match.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut store = PatternStore::new();
    /// store.add_bash_pattern("cargo ").unwrap();
    ///
    /// assert!(store.matches_bash("cargo build"));
    /// assert!(store.matches_bash("cargo test"));
    /// assert!(!store.matches_bash("npm install"));
    /// assert!(!store.matches_bash("cargo build; npm install"));
    /// ```
    pub fn matches_bash(&self, command: &str) -> bool {
        let split = split_command_line(command);
        if split.unmodellable.is_some() || split.segments.is_empty() {
            return false;
        }
        split.segments.iter().all(|statement| {
            self.bash_commands
                .allowed_prefixes
                .iter()
                .any(|prefix| statement.starts_with(prefix))
        })
    }

    /// Check if a file path matches any allowed pattern
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut store = PatternStore::new();
    /// store.add_file_pattern("src/").unwrap();
    ///
    /// assert!(store.matches_file("src/main.rs"));
    /// assert!(store.matches_file("src/lib.rs"));
    /// assert!(!store.matches_file("tests/test.rs"));
    /// ```
    pub fn matches_file(&self, path: &str) -> bool {
        self.file_paths
            .allowed_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix))
    }

    /// Check if a tool is in the always-allow list
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut store = PatternStore::new();
    /// store.add_tool_pattern("read_note").unwrap();
    /// store.add_tool_pattern("fs_*").unwrap();
    ///
    /// assert!(store.matches_tool("read_note"));
    /// assert!(store.matches_tool("fs_read_file"));
    /// assert!(store.matches_tool("fs_write_file"));
    /// assert!(!store.matches_tool("write_note"));
    /// assert!(!store.matches_tool("gh_create_issue"));
    /// ```
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        self.tools.always_allow.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix('*') {
                tool_name.starts_with(prefix)
            } else {
                pattern == tool_name
            }
        })
    }

    /// Merge another pattern store into this one
    ///
    /// Combines patterns from both stores, removing duplicates.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_core::config::PatternStore;
    ///
    /// let mut base = PatternStore::new();
    /// base.add_bash_pattern("cargo ").unwrap();
    ///
    /// let mut overlay = PatternStore::new();
    /// overlay.add_bash_pattern("npm ").unwrap();
    ///
    /// let merged = base.merge(&overlay);
    /// assert!(merged.matches_bash("cargo build"));
    /// assert!(merged.matches_bash("npm install"));
    /// ```
    pub fn merge(&self, other: &PatternStore) -> PatternStore {
        let mut bash_prefixes: HashSet<String> = self
            .bash_commands
            .allowed_prefixes
            .iter()
            .cloned()
            .collect();
        bash_prefixes.extend(other.bash_commands.allowed_prefixes.iter().cloned());

        let mut file_prefixes: HashSet<String> =
            self.file_paths.allowed_prefixes.iter().cloned().collect();
        file_prefixes.extend(other.file_paths.allowed_prefixes.iter().cloned());

        let mut tool_names: HashSet<String> = self.tools.always_allow.iter().cloned().collect();
        tool_names.extend(other.tools.always_allow.iter().cloned());

        PatternStore {
            bash_commands: BashPatterns {
                allowed_prefixes: bash_prefixes.into_iter().collect(),
            },
            file_paths: FilePatterns {
                allowed_prefixes: file_prefixes.into_iter().collect(),
            },
            tools: ToolPatterns {
                always_allow: tool_names.into_iter().collect(),
            },
        }
    }

    /// Get the path to the whitelists.d directory
    pub fn whitelists_dir() -> PathBuf {
        // SAFETY: dirs::config_dir() fallback ensures we always have a valid path
        Self::whitelists_dir_in(&dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config")))
    }

    /// The whitelists.d directory under the config home `config_home`.
    pub fn whitelists_dir_in(config_home: &Path) -> PathBuf {
        config_home.join("crucible").join("whitelists.d")
    }

    /// Generate a project hash from the project path
    ///
    /// Uses a simple hash to create a unique identifier for each project.
    pub fn project_hash(project_path: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        project_path.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Get the full path to the pattern file for a project
    fn pattern_file_path(project_path: &str) -> PathBuf {
        Self::project_file_in(&Self::whitelists_dir(), project_path)
    }

    /// The project store file under `dir`.
    pub fn project_file_in(dir: &Path, project_path: &str) -> PathBuf {
        let hash = Self::project_hash(project_path);
        dir.join(format!("{}.toml", hash))
    }

    /// The user-wide store file under `dir`. The name cannot collide with a
    /// project file, because a project file name is sixteen hex digits.
    pub fn user_file_in(dir: &Path) -> PathBuf {
        dir.join("user.toml")
    }

    /// The file a grant at `scope` persists to under `dir`. `Once` and
    /// `Session` grants are not persisted.
    pub fn store_file_in(
        dir: &Path,
        scope: PermissionScope,
        project_path: &str,
    ) -> Option<PathBuf> {
        match scope {
            PermissionScope::Project => Some(Self::project_file_in(dir, project_path)),
            PermissionScope::User => Some(Self::user_file_in(dir)),
            PermissionScope::Once | PermissionScope::Session => None,
        }
    }

    /// Validate that a pattern is not too permissive
    fn validate_pattern(pattern: &str) -> PatternResult<()> {
        if pattern.starts_with('*') {
            return Err(PatternError::TooPermissive(pattern.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_store_is_empty() {
        let store = PatternStore::new();
        assert!(store.bash_commands.allowed_prefixes.is_empty());
        assert!(store.file_paths.allowed_prefixes.is_empty());
        assert!(store.tools.always_allow.is_empty());
    }

    #[test]
    fn add_bash_pattern_works() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("cargo build").unwrap();
        store.add_bash_pattern("npm install").unwrap();

        assert_eq!(store.bash_commands.allowed_prefixes.len(), 2);
        assert!(store
            .bash_commands
            .allowed_prefixes
            .contains(&"cargo build".to_string()));
        assert!(store
            .bash_commands
            .allowed_prefixes
            .contains(&"npm install".to_string()));
    }

    #[test]
    fn add_file_pattern_works() {
        let mut store = PatternStore::new();
        store.add_file_pattern("src/").unwrap();
        store.add_file_pattern("tests/").unwrap();

        assert_eq!(store.file_paths.allowed_prefixes.len(), 2);
        assert!(store
            .file_paths
            .allowed_prefixes
            .contains(&"src/".to_string()));
        assert!(store
            .file_paths
            .allowed_prefixes
            .contains(&"tests/".to_string()));
    }

    #[test]
    fn add_tool_pattern_works() {
        let mut store = PatternStore::new();
        store.add_tool_pattern("read_note").unwrap();
        store.add_tool_pattern("grep_notes").unwrap();

        assert_eq!(store.tools.always_allow.len(), 2);
        assert!(store.tools.always_allow.contains(&"read_note".to_string()));
        assert!(store.tools.always_allow.contains(&"grep_notes".to_string()));
    }

    #[test]
    fn rejects_star_pattern() {
        let mut store = PatternStore::new();

        assert!(matches!(
            store.add_bash_pattern("*"),
            Err(PatternError::TooPermissive(_))
        ));
        assert!(matches!(
            store.add_file_pattern("*"),
            Err(PatternError::TooPermissive(_))
        ));
        assert!(matches!(
            store.add_tool_pattern("*"),
            Err(PatternError::TooPermissive(_))
        ));

        // Also reject patterns starting with *
        assert!(matches!(
            store.add_bash_pattern("*.rs"),
            Err(PatternError::TooPermissive(_))
        ));
    }

    #[test]
    fn matches_bash_works() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("cargo ").unwrap();
        store.add_bash_pattern("git ").unwrap();

        assert!(store.matches_bash("cargo build"));
        assert!(store.matches_bash("cargo test --release"));
        assert!(store.matches_bash("git status"));
        assert!(!store.matches_bash("npm install"));
        assert!(!store.matches_bash("cargotest")); // No space, shouldn't match "cargo "
    }

    /// A saved prefix allows one statement. A line that chains a second statement after it
    /// only passes when every statement matches a prefix.
    #[test]
    fn matches_bash_checks_every_chained_statement() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("git ").unwrap();

        for chained in [
            "git log; curl evil",
            "git log && curl evil",
            "git log || curl evil",
            "git log | curl evil",
            "git log & curl evil",
            "git log\ncurl evil",
            "git log $(curl evil)",
            "git log `curl evil`",
            "git log <(curl evil)",
        ] {
            assert!(!store.matches_bash(chained), "matches_bash({chained:?})");
        }

        assert!(store.matches_bash("git fetch && git log"));
        assert!(store.matches_bash("git log | git status"));
        assert!(store.matches_bash("git log 'a; b'"));
        assert!(!store.matches_bash(""));
    }

    #[test]
    fn matches_file_works() {
        let mut store = PatternStore::new();
        store.add_file_pattern("src/").unwrap();
        store.add_file_pattern("tests/").unwrap();

        assert!(store.matches_file("src/main.rs"));
        assert!(store.matches_file("src/lib/mod.rs"));
        assert!(store.matches_file("tests/integration.rs"));
        assert!(!store.matches_file("docs/readme.md"));
    }

    #[test]
    fn matches_tool_works() {
        let mut store = PatternStore::new();
        store.add_tool_pattern("read_note").unwrap();

        assert!(store.matches_tool("read_note"));
        assert!(!store.matches_tool("write_note"));
        assert!(!store.matches_tool("read_note_extended")); // Exact match only
    }

    #[test]
    fn no_duplicate_patterns() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("cargo ").unwrap();
        store.add_bash_pattern("cargo ").unwrap();
        store.add_bash_pattern("cargo ").unwrap();

        assert_eq!(store.bash_commands.allowed_prefixes.len(), 1);
    }

    #[test]
    fn merge_combines_patterns() {
        let mut base = PatternStore::new();
        base.add_bash_pattern("cargo ").unwrap();
        base.add_file_pattern("src/").unwrap();
        base.add_tool_pattern("read_note").unwrap();

        let mut overlay = PatternStore::new();
        overlay.add_bash_pattern("npm ").unwrap();
        overlay.add_bash_pattern("cargo ").unwrap(); // duplicate
        overlay.add_file_pattern("tests/").unwrap();
        overlay.add_tool_pattern("grep_notes").unwrap();

        let merged = base.merge(&overlay);

        // Bash patterns merged and deduplicated
        assert_eq!(merged.bash_commands.allowed_prefixes.len(), 2);
        assert!(merged.matches_bash("cargo build"));
        assert!(merged.matches_bash("npm install"));

        // File patterns merged
        assert_eq!(merged.file_paths.allowed_prefixes.len(), 2);
        assert!(merged.matches_file("src/main.rs"));
        assert!(merged.matches_file("tests/test.rs"));

        // Tool patterns merged
        assert_eq!(merged.tools.always_allow.len(), 2);
        assert!(merged.matches_tool("read_note"));
        assert!(merged.matches_tool("grep_notes"));
    }

    #[test]
    fn project_hash_is_deterministic() {
        let hash1 = PatternStore::project_hash("/home/user/project");
        let hash2 = PatternStore::project_hash("/home/user/project");
        let hash3 = PatternStore::project_hash("/home/user/other-project");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16); // 16 hex chars
    }

    #[test]
    fn serializes_to_toml() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("npm install").unwrap();
        store.add_bash_pattern("cargo build").unwrap();
        store.add_file_pattern("src/").unwrap();
        store.add_tool_pattern("read_note").unwrap();

        let toml_str = toml::to_string_pretty(&store).unwrap();

        // Verify structure
        assert!(toml_str.contains("[bash_commands]"));
        assert!(toml_str.contains("[file_paths]"));
        assert!(toml_str.contains("[tools]"));
        assert!(toml_str.contains("npm install"));
        assert!(toml_str.contains("cargo build"));
        assert!(toml_str.contains("src/"));
        assert!(toml_str.contains("read_note"));
    }

    #[test]
    fn deserializes_from_toml() {
        let toml_str = r#"
[bash_commands]
allowed_prefixes = ["npm install", "cargo build", "git "]

[file_paths]
allowed_prefixes = ["src/", "tests/"]

[tools]
always_allow = ["read_note", "grep_notes"]
"#;

        let store: PatternStore = toml::from_str(toml_str).unwrap();

        assert_eq!(store.bash_commands.allowed_prefixes.len(), 3);
        assert!(store.matches_bash("npm install lodash"));
        assert!(store.matches_bash("cargo build --release"));
        assert!(store.matches_bash("git status"));

        assert_eq!(store.file_paths.allowed_prefixes.len(), 2);
        assert!(store.matches_file("src/main.rs"));
        assert!(store.matches_file("tests/test.rs"));

        assert_eq!(store.tools.always_allow.len(), 2);
        assert!(store.matches_tool("read_note"));
        assert!(store.matches_tool("grep_notes"));
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().to_str().unwrap();

        // Create and save a store
        let mut store = PatternStore::new();
        store.add_bash_pattern("cargo ").unwrap();
        store.add_file_pattern("src/").unwrap();
        store.add_tool_pattern("read_note").unwrap();

        // Override the whitelists dir for testing
        let whitelists_dir = temp_dir.path().join("whitelists.d");
        std::fs::create_dir_all(&whitelists_dir).unwrap();

        let hash = PatternStore::project_hash(project_path);
        let file_path = whitelists_dir.join(format!("{}.toml", hash));

        // Save manually to temp location
        let content = toml::to_string_pretty(&store).unwrap();
        std::fs::write(&file_path, &content).unwrap();

        // Load and verify
        let loaded_content = std::fs::read_to_string(&file_path).unwrap();
        let loaded: PatternStore = toml::from_str(&loaded_content).unwrap();

        assert!(loaded.matches_bash("cargo build"));
        assert!(loaded.matches_file("src/main.rs"));
        assert!(loaded.matches_tool("read_note"));
    }

    #[test]
    fn save_file_then_load_file_round_trips() {
        let temp_dir = TempDir::new().unwrap();
        let file = PatternStore::user_file_in(&temp_dir.path().join("whitelists.d"));
        let mut store = PatternStore::new();
        store.add_bash_pattern("cargo ").unwrap();

        store.save_file(&file).unwrap();

        assert_eq!(PatternStore::load_file(&file).unwrap(), store);
    }

    /// The store holds permission whitelists. `fs::write` follows the umask,
    /// which can leave the file readable by the group or by the world.
    #[cfg(unix)]
    #[test]
    fn save_file_writes_the_store_only_owner_readable() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = TempDir::new().unwrap();
        let file = PatternStore::user_file_in(&temp_dir.path().join("whitelists.d"));
        let mut store = PatternStore::new();
        store.add_bash_pattern("cargo ").unwrap();

        store.save_file(&file).unwrap();

        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the whitelist file must be private");
    }

    #[test]
    fn load_sync_returns_empty_for_missing_file() {
        // Use a path that definitely doesn't exist
        let store = PatternStore::load_sync("/nonexistent/path/that/does/not/exist").unwrap();
        assert!(store.bash_commands.allowed_prefixes.is_empty());
        assert!(store.file_paths.allowed_prefixes.is_empty());
        assert!(store.tools.always_allow.is_empty());
    }

    #[test]
    fn whitelists_dir_uses_xdg() {
        let dir = PatternStore::whitelists_dir();
        // Should end with crucible/whitelists.d
        assert!(dir.ends_with("crucible/whitelists.d"));
    }
}
