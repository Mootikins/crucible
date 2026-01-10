//! Plugin loader for discovering and loading Rune plugins
//!
//! Plugins are Rune scripts that define an `init()` function returning
//! registration data (handlers, tools, etc.).

use crate::plugin_types::{PluginManifest, RegisteredHook};
use crate::{RuneError, RuneExecutor};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Plugin loader that discovers and loads Rune plugins
pub struct PluginLoader {
    /// Rune executor for compiling and running scripts
    executor: RuneExecutor,
    /// Registered handlers from all loaded plugins
    hooks: Vec<RegisteredHook>,
    /// Base directory to search for plugins
    plugin_dir: PathBuf,
}

impl PluginLoader {
    /// Create a new plugin loader for the given directory
    pub fn new(plugin_dir: impl AsRef<Path>) -> Result<Self, RuneError> {
        Ok(Self {
            executor: RuneExecutor::new()?,
            hooks: Vec::new(),
            plugin_dir: plugin_dir.as_ref().to_path_buf(),
        })
    }

    /// Load all plugins from the plugin directory (including subdirectories)
    pub async fn load_plugins(&mut self) -> Result<(), RuneError> {
        if !self.plugin_dir.exists() {
            debug!(
                "Plugin directory does not exist: {}",
                self.plugin_dir.display()
            );
            return Ok(());
        }

        let pattern = format!("{}/**/*.rn", self.plugin_dir.display());
        debug!("Loading plugins from pattern: {}", pattern);

        let paths: Vec<PathBuf> = glob::glob(&pattern)
            .map_err(|e| RuneError::Discovery(format!("Invalid glob pattern: {}", e)))?
            .filter_map(|r| {
                match &r {
                    Ok(p) => debug!("Found plugin file: {}", p.display()),
                    Err(e) => warn!("Glob error: {}", e),
                }
                r.ok()
            })
            .collect();

        debug!("Found {} plugin files to load", paths.len());

        for path in paths {
            if let Err(e) = self.load_plugin(&path).await {
                warn!("Failed to load plugin {}: {}", path.display(), e);
                // Continue loading other plugins
            }
        }

        debug!("Loaded {} handlers from plugins", self.hooks.len());
        Ok(())
    }

    /// Load a single plugin file
    async fn load_plugin(&mut self, path: &Path) -> Result<(), RuneError> {
        debug!("Loading plugin: {}", path.display());

        // Read and compile the script
        let source = std::fs::read_to_string(path)
            .map_err(|e| RuneError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

        let unit = match self
            .executor
            .compile(path.to_string_lossy().as_ref(), &source)
        {
            Ok(u) => u,
            Err(e) => {
                debug!("Failed to compile {}: {}", path.display(), e);
                return Err(e);
            }
        };

        // Try to call init() - if it doesn't exist, skip this plugin
        let init_result = match self.executor.call_function(&unit, "init", ()).await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    "Plugin {} has no init() or init() failed: {}",
                    path.display(),
                    e
                );
                return Ok(()); // Not an error, just no handlers from this file
            }
        };

        // Parse the manifest
        let manifest = PluginManifest::from_json(&init_result).map_err(RuneError::Conversion)?;

        // Register handlers
        for hook_config in manifest.hooks {
            match hook_config.to_registered_hook(path.to_path_buf(), Some(unit.clone())) {
                Ok(hook) => {
                    debug!(
                        "Registered handler: {} on {} -> {}",
                        hook.event_type, hook.pattern, hook.handler_name
                    );
                    self.hooks.push(hook);
                }
                Err(e) => {
                    warn!("Invalid handler in {}: {}", path.display(), e);
                }
            }
        }

        Ok(())
    }

    /// Get all registered handlers
    pub fn hooks(&self) -> &[RegisteredHook] {
        &self.hooks
    }

    /// Get handlers that match an event type and name
    pub fn get_matching_hooks(&self, event_type: &str, name: &str) -> Vec<&RegisteredHook> {
        self.hooks
            .iter()
            .filter(|h| h.matches(event_type, name))
            .collect()
    }

    /// Get the executor (for calling hook handlers)
    pub fn executor(&self) -> &RuneExecutor {
        &self.executor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_plugin(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    async fn test_loader_empty_dir() {
        let temp = TempDir::new().unwrap();
        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();
        assert!(loader.hooks().is_empty());
    }

    #[tokio::test]
    async fn test_loader_single_plugin() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(
            temp.path(),
            "test_plugin.rn",
            r#"pub fn init() {
    #{
        hooks: [
            #{ event: "tool_result", pattern: "just_*", handler: "filter" }
        ]
    }
}

pub fn filter(ctx, event) {
    event
}"#,
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        assert_eq!(loader.hooks().len(), 1);
        assert_eq!(loader.hooks()[0].handler_name, "filter");
    }

    #[tokio::test]
    async fn test_loader_parses_hooks_from_init() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(
            temp.path(),
            "multi_hook.rn",
            r#"
pub fn init() {
    #{
        hooks: [
            #{ event: "tool_result", pattern: "just_test*", handler: "filter_test" },
            #{ event: "tool_result", pattern: "just_build*", handler: "filter_build" }
        ]
    }
}

pub fn filter_test(ctx, event) { event }
pub fn filter_build(ctx, event) { event }
"#,
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        assert_eq!(loader.hooks().len(), 2);
    }

    #[tokio::test]
    async fn test_loader_skips_no_init() {
        let temp = TempDir::new().unwrap();
        // Plugin without init() function
        create_test_plugin(
            temp.path(),
            "no_init.rn",
            r#"
pub fn some_other_function() {
    42
}
"#,
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        // Should not crash, just skip
        assert!(loader.hooks().is_empty());
    }

    #[tokio::test]
    async fn test_loader_handles_compile_error() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(
            temp.path(),
            "bad_syntax.rn",
            "this is not valid rune code {{{{",
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        // Should not crash, just skip bad plugins
        loader.load_plugins().await.unwrap();

        assert!(loader.hooks().is_empty());
    }

    #[tokio::test]
    async fn test_loader_multiple_plugins() {
        let temp = TempDir::new().unwrap();

        create_test_plugin(
            temp.path(),
            "plugin1.rn",
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "a*", handler: "h1" }] }
}
pub fn h1(ctx, e) { e }
"#,
        );

        create_test_plugin(
            temp.path(),
            "plugin2.rn",
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "b*", handler: "h2" }] }
}
pub fn h2(ctx, e) { e }
"#,
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        assert_eq!(loader.hooks().len(), 2);
    }

    #[tokio::test]
    async fn test_get_matching_hooks_filters_correctly() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(
            temp.path(),
            "test.rn",
            r#"
pub fn init() {
    #{
        hooks: [
            #{ event: "tool_result", pattern: "just_test*", handler: "h1" },
            #{ event: "tool_result", pattern: "just_build*", handler: "h2" },
            #{ event: "note_changed", pattern: "*", handler: "h3" }
        ]
    }
}
pub fn h1(ctx, e) { e }
pub fn h2(ctx, e) { e }
pub fn h3(ctx, e) { e }
"#,
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        let matches = loader.get_matching_hooks("tool_result", "just_test_verbose");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler_name, "h1");

        let matches = loader.get_matching_hooks("tool_result", "just_build");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler_name, "h2");

        let matches = loader.get_matching_hooks("note_changed", "any_note");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].handler_name, "h3");

        let matches = loader.get_matching_hooks("tool_result", "just_clean");
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn test_loader_subdirectories() {
        let temp = TempDir::new().unwrap();
        let subdir = temp.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        create_test_plugin(
            &subdir,
            "nested.rn",
            r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "h" }] }
}
pub fn h(ctx, e) { e }
"#,
        );

        let mut loader = PluginLoader::new(temp.path()).unwrap();
        loader.load_plugins().await.unwrap();

        assert_eq!(loader.hooks().len(), 1);
    }
}
