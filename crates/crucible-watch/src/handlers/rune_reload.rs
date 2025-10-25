//! Integration handler for hot-reloading Rune tool scripts.

use crate::{events::FileEvent, traits::EventHandler, error::{Error, Result}};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Handler for automatically reloading Rune tool scripts when they change.
pub struct RuneReloadHandler {
    /// Registry to notify when scripts change
    registry: Option<Arc<dyn RuneRegistry>>,
    /// Supported file extensions for Rune scripts
    supported_extensions: Vec<String>,
    /// Debounce delay for reload operations
    reload_debounce: std::time::Duration,
    /// Whether to validate scripts before reload
    validate_before_reload: bool,
}

/// Trait for Rune registries that can be notified of script changes.
#[async_trait]
pub trait RuneRegistry: Send + Sync {
    /// Reload a specific Rune script.
    async fn reload_script(&self, script_path: &PathBuf) -> Result<()>;

    /// Validate a Rune script.
    async fn validate_script(&self, script_path: &PathBuf) -> Result<bool>;

    /// Get all loaded scripts.
    async fn loaded_scripts(&self) -> Result<Vec<PathBuf>>;

    /// Get the registry name.
    fn name(&self) -> &'static str;
}

impl RuneReloadHandler {
    /// Create a new Rune reload handler.
    pub fn new() -> Result<Self> {
        Ok(Self {
            registry: None,
            supported_extensions: vec!["rune".to_string()],
            reload_debounce: std::time::Duration::from_millis(200),
            validate_before_reload: true,
        })
    }

    /// Create a handler with a Rune registry.
    pub fn with_registry<R: RuneRegistry + 'static>(registry: Arc<R>) -> Result<Self> {
        let mut handler = Self::new()?;
        handler.registry = Some(registry);
        Ok(handler)
    }

    /// Set the supported file extensions.
    pub fn with_supported_extensions(mut self, extensions: Vec<String>) -> Self {
        self.supported_extensions = extensions;
        self
    }

    /// Set the debounce delay for reload operations.
    pub fn with_debounce(mut self, debounce: std::time::Duration) -> Self {
        self.reload_debounce = debounce;
        self
    }

    /// Enable or disable script validation before reload.
    pub fn with_validation(mut self, validate: bool) -> Self {
        self.validate_before_reload = validate;
        self
    }

    fn should_reload_script(&self, path: &PathBuf) -> bool {
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                return self.supported_extensions.contains(&ext_str.to_lowercase());
            }
        }
        false
    }

    async fn get_registry(&self) -> Result<Arc<dyn RuneRegistry>> {
        self.registry.as_ref()
            .cloned()
            .ok_or_else(|| Error::Config("Rune registry not configured".to_string()))
    }

    async fn reload_script(&self, path: &PathBuf) -> Result<()> {
        debug!("Reloading Rune script: {}", path.display());

        let registry = self.get_registry().await?;

        // Validate script if enabled
        if self.validate_before_reload {
            debug!("Validating Rune script: {}", path.display());
            match registry.validate_script(path).await {
                Ok(true) => {
                    debug!("Script validation passed: {}", path.display());
                }
                Ok(false) => {
                    warn!("Script validation failed, skipping reload: {}", path.display());
                    return Ok(());
                }
                Err(e) => {
                    warn!("Script validation error: {}, skipping reload: {}", e, path.display());
                    return Ok(());
                }
            }
        }

        // Perform the reload
        registry.reload_script(path)
            .await
            .map_err(|e| Error::Handler(format!("Failed to reload Rune script: {}", e)))?;

        info!("Successfully reloaded Rune script: {}", path.display());
        Ok(())
    }

    async fn handle_script_deletion(&self, path: &PathBuf) -> Result<()> {
        debug!("Handling deletion of Rune script: {}", path.display());

        let registry = self.get_registry().await?;
        let loaded_scripts = registry.loaded_scripts().await?;

        if loaded_scripts.contains(path) {
            // Script was loaded, now it's deleted - we should unload it
            // This would depend on the registry's capabilities
            info!("Rune script was deleted: {}", path.display());
            // TODO: Add unload capability to RuneRegistry trait
        }

        Ok(())
    }
}

#[async_trait]
impl EventHandler for RuneReloadHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        debug!("Rune reload handler processing event: {:?}", event.kind);

        match event.kind {
            crate::events::FileEventKind::Created | crate::events::FileEventKind::Modified => {
                if let Err(e) = self.reload_script(&event.path).await {
                    error!("Failed to reload Rune script {}: {}", event.path.display(), e);
                    return Err(e);
                }
            }
            crate::events::FileEventKind::Deleted => {
                if let Err(e) = self.handle_script_deletion(&event.path).await {
                    error!("Failed to handle Rune script deletion {}: {}", event.path.display(), e);
                    return Err(e);
                }
            }
            crate::events::FileEventKind::Moved { from, to } => {
                // Handle script move
                debug!("Rune script moved from {} to {}", from.display(), to.display());

                // Remove old script
                if let Err(e) = self.handle_script_deletion(&from).await {
                    warn!("Failed to handle moved Rune script removal {}: {}", from.display(), e);
                }

                // Load new script
                if let Err(e) = self.reload_script(&to).await {
                    error!("Failed to reload moved Rune script {}: {}", to.display(), e);
                    return Err(e);
                }
            }
            crate::events::FileEventKind::Batch(_) => {
                warn!("Batch events not yet supported by Rune reload handler");
            }
            crate::events::FileEventKind::Unknown(_) => {
                debug!("Unknown event type, skipping");
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "rune_reload"
    }

    fn priority(&self) -> u32 {
        150 // High priority for hot-reloading
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        // Only handle file events for Rune scripts
        !event.is_dir && self.should_reload_script(&event.path)
    }
}

/// Mock implementation for testing.
#[cfg(test)]
pub struct MockRuneRegistry {
    name: &'static str,
    loaded_scripts: Arc<RwLock<Vec<PathBuf>>>,
}

#[cfg(test)]
impl MockRuneRegistry {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            loaded_scripts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_script(&self, path: PathBuf) {
        let mut scripts = self.loaded_scripts.write().await;
        if !scripts.contains(&path) {
            scripts.push(path);
        }
    }
}

#[cfg(test)]
#[async_trait]
impl RuneRegistry for MockRuneRegistry {
    async fn reload_script(&self, script_path: &PathBuf) -> Result<()> {
        // Add to loaded scripts
        let mut scripts = self.loaded_scripts.write().await;
        if !scripts.contains(script_path) {
            scripts.push(script_path.clone());
        }
        Ok(())
    }

    async fn validate_script(&self, _script_path: &PathBuf) -> Result<bool> {
        // Mock validation - always succeeds for testing
        Ok(true)
    }

    async fn loaded_scripts(&self) -> Result<Vec<PathBuf>> {
        Ok(self.loaded_scripts.read().await.clone())
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{FileEvent, FileEventKind};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_rune_reload_handler() {
        let registry = Arc::new(MockRuneRegistry::new("test_registry"));
        let handler = RuneReloadHandler::with_registry(registry.clone()).unwrap();

        assert_eq!(handler.name(), "rune_reload");
        assert_eq!(handler.priority(), 150);

        let rune_file = PathBuf::from("test.rune");
        let event = FileEvent::new(FileEventKind::Modified, rune_file.clone());

        assert!(handler.can_handle(&event));

        // Handle the event
        handler.handle(event).await.unwrap();

        // Check that script was reloaded
        let loaded_scripts = registry.loaded_scripts().await.unwrap();
        assert!(loaded_scripts.contains(&rune_file));
    }

    #[tokio::test]
    async fn test_supported_extensions() {
        let handler = RuneReloadHandler::new().unwrap();

        assert!(handler.should_reload_script(&PathBuf::from("test.rune")));
        assert!(!handler.should_reload_script(&PathBuf::from("test.md")));
        assert!(!handler.should_reload_script(&PathBuf::from("test")));
    }
}