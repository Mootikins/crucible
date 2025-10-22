//! # Plugin Registry
//!
//! This module implements the PluginRegistry which handles plugin discovery,
//! registration, validation, and lifecycle management of plugin metadata.

use super::config::{PluginManagerConfig, DiscoveryValidationConfig};
use super::error::{PluginError, PluginResult, ErrorContext};
use super::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// ============================================================================
/// PLUGIN REGISTRY TRAIT
/// ============================================================================

#[async_trait]
pub trait PluginRegistry: Send + Sync {
    /// Discover plugins in configured directories
    async fn discover_plugins(&self) -> PluginResult<Vec<PluginManifest>>;

    /// Register a plugin
    async fn register_plugin(&mut self, manifest: PluginManifest) -> PluginResult<String>;

    /// Unregister a plugin
    async fn unregister_plugin(&mut self, plugin_id: &str) -> PluginResult<()>;

    /// Get plugin manifest by ID
    async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginManifest>>;

    /// List all registered plugins
    async fn list_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>>;

    /// List plugins by type
    async fn list_plugins_by_type(&self, plugin_type: PluginType) -> PluginResult<Vec<PluginRegistryEntry>>;

    /// List enabled plugins
    async fn list_enabled_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>>;

    /// Update plugin status
    async fn update_plugin_status(&mut self, plugin_id: &str, status: PluginRegistryStatus) -> PluginResult<()>;

    /// Validate plugin
    async fn validate_plugin(&self, manifest: &PluginManifest) -> PluginResult<PluginValidationResults>;

    /// Get plugin dependencies
    async fn get_plugin_dependencies(&self, plugin_id: &str) -> PluginResult<Vec<PluginDependency>>;

    /// Check for dependency conflicts
    async fn check_dependency_conflicts(&self, plugins: &[String]) -> PluginResult<Vec<VersionConflict>>;

    /// Subscribe to registry events
    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<RegistryEvent>;
}

/// ============================================================================
/// REGISTRY EVENTS
/// ============================================================================

/// Registry event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegistryEvent {
    /// Plugin discovered
    PluginDiscovered { manifest: PluginManifest },
    /// Plugin registered
    PluginRegistered { plugin_id: String },
    /// Plugin unregistered
    PluginUnregistered { plugin_id: String },
    /// Plugin status changed
    PluginStatusChanged { plugin_id: String, old_status: PluginRegistryStatus, new_status: PluginRegistryStatus },
    /// Plugin validation completed
    PluginValidated { plugin_id: String, results: PluginValidationResults },
    /// Dependency conflict detected
    DependencyConflict { conflicts: Vec<VersionConflict> },
    /// Error occurred
    Error { error: String, context: Option<String> },
}

/// ============================================================================
/// DEFAULT PLUGIN REGISTRY IMPLEMENTATION
/// ============================================================================

/// Default implementation of PluginRegistry
#[derive(Debug)]
pub struct DefaultPluginRegistry {
    /// Registry configuration
    config: Arc<PluginManagerConfig>,
    /// Registered plugins
    plugins: Arc<RwLock<HashMap<String, PluginRegistryEntry>>>,
    /// Plugin index by type
    plugins_by_type: Arc<RwLock<HashMap<PluginType, Vec<String>>>>,
    /// Plugin index by status
    plugins_by_status: Arc<RwLock<HashMap<PluginRegistryStatus, Vec<String>>>>,
    /// Event subscribers
    event_subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<RegistryEvent>>>>,
    /// Discovery cache
    discovery_cache: Arc<RwLock<HashMap<PathBuf, SystemTime>>>,
    /// Validation cache
    validation_cache: Arc<RwLock<HashMap<String, (PluginValidationResults, SystemTime)>>>,
}

impl DefaultPluginRegistry {
    /// Create a new registry with configuration
    pub fn new(config: PluginManagerConfig) -> Self {
        Self {
            config: Arc::new(config),
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugins_by_type: Arc::new(RwLock::new(HashMap::new())),
            plugins_by_status: Arc::new(RwLock::new(HashMap::new())),
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
            discovery_cache: Arc::new(RwLock::new(HashMap::new())),
            validation_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish event to subscribers
    async fn publish_event(&self, event: RegistryEvent) {
        let mut subscribers = self.event_subscribers.write().await;
        let mut to_remove = Vec::new();

        for (i, sender) in subscribers.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                to_remove.push(i);
            }
        }

        // Remove dead subscribers
        for i in to_remove.into_iter().rev() {
            subscribers.remove(i);
        }
    }

    /// Update internal indexes
    async fn update_indexes(&self, plugin_id: &str, entry: &PluginRegistryEntry) {
        // Update type index
        {
            let mut type_index = self.plugins_by_type.write().await;
            type_index.entry(entry.manifest.plugin_type.clone())
                .or_insert_with(Vec::new)
                .push(plugin_id.to_string());
        }

        // Update status index
        {
            let mut status_index = self.plugins_by_status.write().await;
            status_index.entry(entry.status.clone())
                .or_insert_with(Vec::new)
                .push(plugin_id.to_string());
        }
    }

    /// Remove from internal indexes
    async fn remove_from_indexes(&self, plugin_id: &str, entry: &PluginRegistryEntry) {
        // Remove from type index
        {
            let mut type_index = self.plugins_by_type.write().await;
            if let Some(plugins) = type_index.get_mut(&entry.manifest.plugin_type) {
                plugins.retain(|id| id != plugin_id);
                if plugins.is_empty() {
                    type_index.remove(&entry.manifest.plugin_type);
                }
            }
        }

        // Remove from status index
        {
            let mut status_index = self.plugins_by_status.write().await;
            if let Some(plugins) = status_index.get_mut(&entry.status) {
                plugins.retain(|id| id != plugin_id);
                if plugins.is_empty() {
                    status_index.remove(&entry.status);
                }
            }
        }
    }

    /// Scan a directory for plugins
    async fn scan_directory(&self, directory: &Path) -> PluginResult<Vec<PluginManifest>> {
        let mut manifests = Vec::new();

        if !directory.exists() {
            warn!("Plugin directory does not exist: {:?}", directory);
            return Ok(manifests);
        }

        info!("Scanning plugin directory: {:?}", directory);

        let mut entries = tokio::fs::read_dir(directory)
            .await
            .map_err(|e| PluginError::discovery(format!("Failed to read directory {:?}: {}", directory, e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| PluginError::discovery(format!("Failed to read directory entry: {}", e)))? {

            let path = entry.path();

            // Check cache
            let should_scan = {
                let metadata = entry.metadata().await
                    .map_err(|e| PluginError::discovery(format!("Failed to read metadata for {:?}: {}", path, e)))?;

                let modified = metadata.modified()
                    .map_err(|e| PluginError::discovery(format!("Failed to get modification time for {:?}: {}", path, e)))?;

                let mut cache = self.discovery_cache.write().await;

                match cache.get(&path) {
                    Some(cached_time) => {
                        if modified > *cached_time {
                            cache.insert(path.clone(), modified);
                            true
                        } else {
                            false
                        }
                    }
                    None => {
                        cache.insert(path.clone(), modified);
                        true
                    }
                }
            };

            if !should_scan {
                continue;
            }

            // Try to discover plugin from this path
            if let Ok(manifest) = self.discover_plugin_at_path(&path).await {
                manifests.push(manifest);
            }
        }

        Ok(manifests)
    }

    /// Discover plugin at a specific path
    async fn discover_plugin_at_path(&self, path: &Path) -> PluginResult<PluginManifest> {
        // Check if it's a manifest file
        if path.is_file() {
            let extension = path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");

            match extension {
                "json" | "yaml" | "yml" => {
                    self.load_manifest_file(path).await
                }
                _ => {
                    // Try to treat as an executable script/binary
                    self.discover_executable_plugin(path).await
                }
            }
        } else if path.is_dir() {
            // Check for manifest file in directory
            let manifest_files = ["plugin.json", "plugin.yaml", "plugin.yml", "crucible-plugin.json"];

            for manifest_file in &manifest_files {
                let manifest_path = path.join(manifest_file);
                if manifest_path.exists() {
                    return self.load_manifest_file(&manifest_path).await;
                }
            }

            // Try to treat directory as plugin
            self.discover_directory_plugin(path).await
        } else {
            Err(PluginError::discovery(format!("Path is neither file nor directory: {:?}", path)))
        }
    }

    /// Load manifest from file
    async fn load_manifest_file(&self, path: &Path) -> PluginResult<PluginManifest> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| PluginError::discovery(format!("Failed to read manifest file {:?}: {}", path, e)))?;

        let manifest: PluginManifest = if path.extension().and_then(|ext| ext.to_str()) == Some("yaml") ||
            path.extension().and_then(|ext| ext.to_str()) == Some("yml") {
            serde_yaml::from_str(&content)
                .map_err(|e| PluginError::discovery(format!("Failed to parse YAML manifest {:?}: {}", path, e)))?
        } else {
            serde_json::from_str(&content)
                .map_err(|e| PluginError::discovery(format!("Failed to parse JSON manifest {:?}: {}", path, e)))?
        };

        // Set entry point if not specified in manifest
        let mut manifest = manifest;
        if manifest.entry_point == PathBuf::new() {
            manifest.entry_point = path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
        }

        debug!("Loaded manifest from {:?}: {} ({})", path, manifest.name, manifest.id);
        Ok(manifest)
    }

    /// Discover executable plugin
    async fn discover_executable_plugin(&self, path: &Path) -> PluginResult<PluginManifest> {
        // Check if file is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(path).await
                .map_err(|e| PluginError::discovery(format!("Failed to read file metadata {:?}: {}", path, e)))?;

            if !metadata.permissions().mode() & 0o111 != 0 {
                return Err(PluginError::discovery(format!("File is not executable: {:?}", path)));
            }
        }

        // Create basic manifest for executable
        let filename = path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| PluginError::discovery("Invalid filename".to_string()))?;

        let plugin_id = format!("executable-{}", filename);
        let plugin_type = if filename.ends_with(".py") {
            PluginType::Python
        } else if filename.ends_with(".js") || filename.ends_with(".node") {
            PluginType::JavaScript
        } else if filename.ends_with(".wasm") {
            PluginType::Wasm
        } else {
            PluginType::Binary
        };

        let manifest = PluginManifest {
            id: plugin_id.clone(),
            name: filename.to_string(),
            description: format!("Auto-discovered {} plugin", filename),
            version: "1.0.0".to_string(),
            plugin_type,
            author: "Auto-discovered".to_string(),
            license: None,
            homepage: None,
            repository: None,
            tags: vec!["auto-discovered".to_string()],
            entry_point: path.to_path_buf(),
            capabilities: vec![],
            permissions: vec![],
            dependencies: vec![],
            resource_limits: ResourceLimits::default(),
            sandbox_config: self.config.security.default_sandbox.clone(),
            environment: HashMap::new(),
            config_schema: None,
            min_crucible_version: None,
            max_crucible_version: None,
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
        };

        debug!("Auto-discovered executable plugin: {} from {:?}", manifest.id, path);
        Ok(manifest)
    }

    /// Discover directory plugin
    async fn discover_directory_plugin(&self, path: &Path) -> PluginResult<PluginManifest> {
        // Look for main entry point files
        let entry_files = [
            "main.rn", "main.rune",     // Rune scripts
            "main.py",                 // Python scripts
            "main.js", "index.js",     // JavaScript
            "main.wasm", "plugin.wasm", // WebAssembly
            "plugin", "main",          // Executables
        ];

        for entry_file in &entry_files {
            let entry_path = path.join(entry_file);
            if entry_path.exists() {
                return self.discover_plugin_at_path(&entry_path).await;
            }
        }

        Err(PluginError::discovery(format!("No entry point found in directory: {:?}", path)))
    }

    /// Validate plugin manifest
    async fn validate_manifest_internal(&self, manifest: &PluginManifest) -> PluginResult<PluginValidationResults> {
        // Check cache first
        {
            let cache = self.validation_cache.read().await;
            if let Some((results, timestamp)) = cache.get(&manifest.id) {
                // Cache valid for 1 hour
                if SystemTime::now().duration_since(*timestamp).unwrap_or(Duration::MAX) < Duration::from_secs(3600) {
                    return Ok(results.clone());
                }
            }
        }

        let mut results = PluginValidationResults {
            valid: true,
            security_validation: SecurityValidationResult {
                passed: true,
                issues: vec![],
                security_level: SecurityLevel::Basic,
                recommendations: vec![],
            },
            dependency_validation: DependencyValidationResult {
                passed: true,
                missing_dependencies: vec![],
                version_conflicts: vec![],
                optional_missing: vec![],
            },
            compatibility_validation: CompatibilityValidationResult {
                passed: true,
                crucible_version_compatible: true,
                platform_compatible: true,
                architecture_compatible: true,
                issues: vec![],
            },
            validated_at: SystemTime::now(),
        };

        // Validate manifest structure
        if let Err(e) = manifest.validate() {
            results.valid = false;
            results.security_validation.passed = false;
            results.security_validation.issues.push(SecurityIssue {
                issue_type: SecurityIssueType::CodeInjection,
                severity: SecuritySeverity::High,
                description: format!("Invalid manifest structure: {}", e),
                location: Some("manifest".to_string()),
                recommendation: Some("Fix manifest structure and required fields".to_string()),
            });
        }

        // Security validation
        self.validate_security(manifest, &mut results).await?;

        // Dependency validation
        self.validate_dependencies(manifest, &mut results).await?;

        // Compatibility validation
        self.validate_compatibility(manifest, &mut results).await?;

        // Cache results
        {
            let mut cache = self.validation_cache.write().await;
            cache.insert(manifest.id.clone(), (results.clone(), SystemTime::now()));
        }

        Ok(results)
    }

    /// Validate security aspects
    async fn validate_security(&self, manifest: &PluginManifest, results: &mut PluginValidationResults) -> PluginResult<()> {
        let config = &self.config.auto_discovery.validation;

        // Check for dangerous capabilities
        for capability in &manifest.capabilities {
            match capability {
                PluginCapability::FileSystem { read_paths, write_paths } => {
                    if config.security_scan {
                        // Check for dangerous file access
                        for path in write_paths {
                            if path.starts_with("/etc") || path.starts_with("/boot") || path.starts_with("/sys") {
                                results.security_validation.passed = false;
                                results.security_validation.issues.push(SecurityIssue {
                                    issue_type: SecurityIssueType::FileSystemAccess,
                                    severity: SecuritySeverity::Critical,
                                    description: format!("Plugin requests write access to sensitive path: {}", path),
                                    location: Some("capabilities".to_string()),
                                    recommendation: Some("Remove access to system directories".to_string()),
                                });
                            }
                        }
                    }
                }
                PluginCapability::Network { allowed_hosts, allowed_ports } => {
                    if config.security_scan {
                        // Check for dangerous network access
                        for host in allowed_hosts {
                            if host == "0.0.0.0" || host == "::" {
                                results.security_validation.issues.push(SecurityIssue {
                                    issue_type: SecurityIssueType::NetworkAccess,
                                    severity: SecuritySeverity::Medium,
                                    description: format!("Plugin requests network access to all interfaces: {}", host),
                                    location: Some("capabilities".to_string()),
                                    recommendation: Some("Specify specific hosts instead of wildcard".to_string()),
                                });
                            }
                        }
                    }
                }
                PluginCapability::SystemCalls { allowed_calls } => {
                    if config.security_scan {
                        // Check for dangerous system calls
                        for syscall in allowed_calls {
                            if syscall.contains("ptrace") || syscall.contains("process_vm") {
                                results.security_validation.passed = false;
                                results.security_validation.issues.push(SecurityIssue {
                                    issue_type: SecurityIssueType::PrivilegeEscalation,
                                    severity: SecuritySeverity::Critical,
                                    description: format!("Plugin requests dangerous system call: {}", syscall),
                                    location: Some("capabilities".to_string()),
                                    recommendation: Some("Remove access to debugging/ptrace syscalls".to_string()),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Determine security level
        if results.security_validation.issues.is_empty() {
            results.security_validation.security_level = SecurityLevel::Basic;
        } else {
            let has_critical = results.security_validation.issues.iter()
                .any(|issue| matches!(issue.severity, SecuritySeverity::Critical));

            if has_critical {
                results.security_validation.security_level = SecurityLevel::Maximum;
            } else {
                results.security_validation.security_level = SecurityLevel::Strict;
            }
        }

        Ok(())
    }

    /// Validate dependencies
    async fn validate_dependencies(&self, manifest: &PluginManifest, results: &mut PluginValidationResults) -> PluginResult<()> {
        for dependency in &manifest.dependencies {
            if !dependency.optional {
                // Check if dependency is available in registry
                let plugins = self.plugins.read().await;
                if !plugins.contains_key(&dependency.name) {
                    results.dependency_validation.passed = false;
                    results.dependency_validation.missing_dependencies.push(dependency.clone());
                }
            }
        }

        Ok(())
    }

    /// Validate compatibility
    async fn validate_compatibility(&self, manifest: &PluginManifest, results: &mut PluginValidationResults) -> PluginResult<()> {
        // Check Crucible version compatibility
        let current_version = env!("CARGO_PKG_VERSION");

        if let Some(min_version) = &manifest.min_crucible_version {
            if !self.is_version_compatible(min_version, current_version) {
                results.compatibility_validation.passed = false;
                results.compatibility_validation.crucible_version_compatible = false;
                results.compatibility_validation.issues.push(
                    format!("Plugin requires Crucible version >= {}, current is {}", min_version, current_version)
                );
            }
        }

        if let Some(max_version) = &manifest.max_crucible_version {
            if !self.is_version_compatible(current_version, max_version) {
                results.compatibility_validation.passed = false;
                results.compatibility_validation.crucible_version_compatible = false;
                results.compatibility_validation.issues.push(
                    format!("Plugin requires Crucible version <= {}, current is {}", max_version, current_version)
                );
            }
        }

        // Check platform compatibility
        let current_platform = std::env::consts::OS;
        // For now, assume all platforms are supported
        results.compatibility_validation.platform_compatible = true;

        // Check architecture compatibility
        let current_arch = std::env::consts::ARCH;
        // For now, assume all architectures are supported
        results.compatibility_validation.architecture_compatible = true;

        Ok(())
    }

    /// Simple version compatibility check
    fn is_version_compatible(&self, required: &str, current: &str) -> bool {
        // This is a simplified version comparison
        // In a real implementation, you'd use semantic versioning
        required <= current
    }
}

#[async_trait]
impl PluginRegistry for DefaultPluginRegistry {
    async fn discover_plugins(&self) -> PluginResult<Vec<PluginManifest>> {
        let mut all_manifests = Vec::new();

        for directory in &self.config.plugin_directories {
            match self.scan_directory(directory).await {
                Ok(manifests) => {
                    info!("Discovered {} plugins in {:?}", manifests.len(), directory);
                    all_manifests.extend(manifests);
                }
                Err(e) => {
                    error!("Failed to scan directory {:?}: {}", directory, e);
                    self.publish_event(RegistryEvent::Error {
                        error: format!("Failed to scan directory: {}", e),
                        context: Some(directory.to_string_lossy().to_string()),
                    }).await;
                }
            }
        }

        // Remove duplicates (by ID)
        let mut unique_manifests = HashMap::new();
        for manifest in all_manifests {
            unique_manifests.insert(manifest.id.clone(), manifest);
        }

        let manifests: Vec<PluginManifest> = unique_manifests.into_values().collect();

        // Publish discovery events
        for manifest in &manifests {
            self.publish_event(RegistryEvent::PluginDiscovered {
                manifest: manifest.clone(),
            }).await;
        }

        Ok(manifests)
    }

    async fn register_plugin(&mut self, manifest: PluginManifest) -> PluginResult<String> {
        let plugin_id = manifest.id.clone();

        // Validate plugin first
        let validation_results = self.validate_plugin(&manifest).await?;
        if !validation_results.valid && self.config.auto_discovery.validation.strict {
            return Err(PluginError::validation(format!(
                "Plugin validation failed for {}: {} issues found",
                plugin_id,
                validation_results.security_validation.issues.len()
            )));
        }

        // Check if plugin already exists
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(&plugin_id) {
                return Err(PluginError::registry(format!("Plugin {} is already registered", plugin_id)));
            }
        }

        // Create registry entry
        let entry = PluginRegistryEntry {
            manifest: manifest.clone(),
            install_path: manifest.entry_point.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
            installed_at: SystemTime::now(),
            status: PluginRegistryStatus::Installed,
            validation_results: Some(validation_results.clone()),
            instance_ids: Vec::new(),
        };

        // Register plugin
        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(plugin_id.clone(), entry);
        }

        // Update indexes
        let plugins = self.plugins.read().await;
        if let Some(entry) = plugins.get(&plugin_id) {
            self.update_indexes(&plugin_id, entry).await;
        }

        // Publish event
        self.publish_event(RegistryEvent::PluginRegistered {
            plugin_id: plugin_id.clone(),
        }).await;

        self.publish_event(RegistryEvent::PluginValidated {
            plugin_id: plugin_id.clone(),
            results: validation_results,
        }).await;

        info!("Registered plugin: {} ({})", plugin_id, manifest.name);
        Ok(plugin_id)
    }

    async fn unregister_plugin(&mut self, plugin_id: &str) -> PluginResult<()> {
        // Check if plugin exists
        let entry = {
            let mut plugins = self.plugins.write().await;
            plugins.remove(plugin_id)
                .ok_or_else(|| PluginError::registry(format!("Plugin {} not found", plugin_id)))?
        };

        // Check if plugin has active instances
        if !entry.instance_ids.is_empty() {
            return Err(PluginError::lifecycle(format!(
                "Cannot unregister plugin {} - has {} active instances",
                plugin_id,
                entry.instance_ids.len()
            )));
        }

        // Remove from indexes
        self.remove_from_indexes(plugin_id, &entry).await;

        // Publish event
        self.publish_event(RegistryEvent::PluginUnregistered {
            plugin_id: plugin_id.to_string(),
        }).await;

        info!("Unregistered plugin: {}", plugin_id);
        Ok(())
    }

    async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginManifest>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.get(plugin_id).map(|entry| entry.manifest.clone()))
    }

    async fn list_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.values().cloned().collect())
    }

    async fn list_plugins_by_type(&self, plugin_type: PluginType) -> PluginResult<Vec<PluginRegistryEntry>> {
        let plugins = self.plugins.read().await;
        let type_index = self.plugins_by_type.read().await;

        if let Some(plugin_ids) = type_index.get(&plugin_type) {
            let mut entries = Vec::new();
            for plugin_id in plugin_ids {
                if let Some(entry) = plugins.get(plugin_id) {
                    entries.push(entry.clone());
                }
            }
            Ok(entries)
        } else {
            Ok(Vec::new())
        }
    }

    async fn list_enabled_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>> {
        let plugins = self.plugins.read().await;
        let status_index = self.plugins_by_status.read().await;

        let mut entries = Vec::new();

        // Collect enabled plugins
        for status in [PluginRegistryStatus::Installed] {
            if let Some(plugin_ids) = status_index.get(&status) {
                for plugin_id in plugin_ids {
                    if let Some(entry) = plugins.get(plugin_id) {
                        entries.push(entry.clone());
                    }
                }
            }
        }

        Ok(entries)
    }

    async fn update_plugin_status(&mut self, plugin_id: &str, status: PluginRegistryStatus) -> PluginResult<()> {
        let old_status = {
            let mut plugins = self.plugins.write().await;
            let entry = plugins.get_mut(plugin_id)
                .ok_or_else(|| PluginError::registry(format!("Plugin {} not found", plugin_id)))?;

            let old_status = entry.status.clone();
            entry.status = status.clone();
            old_status
        };

        // Update status index
        {
            let plugins = self.plugins.read().await;
            if let Some(entry) = plugins.get(plugin_id) {
                self.remove_from_indexes(plugin_id, entry).await;
                self.update_indexes(plugin_id, entry).await;
            }
        }

        // Publish event
        self.publish_event(RegistryEvent::PluginStatusChanged {
            plugin_id: plugin_id.to_string(),
            old_status,
            new_status: status,
        }).await;

        Ok(())
    }

    async fn validate_plugin(&self, manifest: &PluginManifest) -> PluginResult<PluginValidationResults> {
        self.validate_manifest_internal(manifest).await
    }

    async fn get_plugin_dependencies(&self, plugin_id: &str) -> PluginResult<Vec<PluginDependency>> {
        let plugins = self.plugins.read().await;
        let entry = plugins.get(plugin_id)
            .ok_or_else(|| PluginError::registry(format!("Plugin {} not found", plugin_id)))?;

        Ok(entry.manifest.dependencies.clone())
    }

    async fn check_dependency_conflicts(&self, plugins: &[String]) -> PluginResult<Vec<VersionConflict>> {
        let mut conflicts = Vec::new();
        let plugins_map = self.plugins.read().await;

        // Collect all dependencies
        let mut all_dependencies = HashMap::new();
        for plugin_id in plugins {
            if let Some(entry) = plugins_map.get(plugin_id) {
                for dep in &entry.manifest.dependencies {
                    all_dependencies.entry(&dep.name)
                        .or_insert_with(Vec::new)
                        .push((plugin_id, dep));
                }
            }
        }

        // Check for conflicts
        for (dep_name, deps) in all_dependencies {
            if deps.len() > 1 {
                // Multiple plugins depend on the same dependency
                // Check for version conflicts
                for (i, (plugin_a, dep_a)) in deps.iter().enumerate() {
                    for (plugin_b, dep_b) in deps.iter().skip(i + 1) {
                        if let (Some(version_a), Some(version_b)) = (&dep_a.version, &dep_b.version) {
                            if version_a != version_b {
                                conflicts.push(VersionConflict {
                                    dependency_name: dep_name.clone(),
                                    required_version: version_a.clone(),
                                    found_version: version_b.clone(),
                                    conflict_description: format!(
                                        "Plugin {} requires {} {}, but plugin {} requires {} {}",
                                        plugin_a, dep_name, version_a,
                                        plugin_b, dep_name, version_b
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }

        if !conflicts.is_empty() {
            self.publish_event(RegistryEvent::DependencyConflict {
                conflicts: conflicts.clone(),
            }).await;
        }

        Ok(conflicts)
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<RegistryEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.event_subscribers.write().await;
        subscribers.push(tx);
        rx
    }
}

/// ============================================================================
/// PLUGIN INSTALLER
/// ============================================================================

/// Plugin installer for installing plugins from various sources
#[derive(Debug)]
pub struct PluginInstaller {
    registry: Arc<RwLock<dyn PluginRegistry>>,
    config: Arc<PluginManagerConfig>,
}

impl PluginInstaller {
    /// Create a new plugin installer
    pub fn new(registry: Arc<RwLock<dyn PluginRegistry>>, config: PluginManagerConfig) -> Self {
        Self {
            registry,
            config: Arc::new(config),
        }
    }

    /// Install plugin from manifest file
    pub async fn install_from_manifest(&self, manifest_path: &Path) -> PluginResult<String> {
        info!("Installing plugin from manifest: {:?}", manifest_path);

        let manifest = self.load_manifest(manifest_path).await?;
        self.install_plugin(manifest).await
    }

    /// Install plugin from directory
    pub async fn install_from_directory(&self, plugin_dir: &Path) -> PluginResult<String> {
        info!("Installing plugin from directory: {:?}", plugin_dir);

        // Look for manifest in directory
        let manifest_files = ["plugin.json", "plugin.yaml", "plugin.yml"];
        let mut manifest_path = None;

        for manifest_file in &manifest_files {
            let path = plugin_dir.join(manifest_file);
            if path.exists() {
                manifest_path = Some(path);
                break;
            }
        }

        if let Some(manifest_path) = manifest_path {
            self.install_from_manifest(&manifest_path).await
        } else {
            // Try to auto-discover
            let discovery = DefaultPluginRegistry::new(self.config.as_ref().clone());
            if let Ok(manifest) = discovery.discover_plugin_at_path(plugin_dir).await {
                self.install_plugin(manifest).await
            } else {
                Err(PluginError::installation(format!(
                    "No manifest found in directory and auto-discovery failed: {:?}",
                    plugin_dir
                )))
            }
        }
    }

    /// Install plugin from URL
    pub async fn install_from_url(&self, url: &str) -> PluginResult<String> {
        info!("Installing plugin from URL: {}", url);

        // Download plugin
        let temp_dir = std::env::temp_dir().join(format!("crucible-plugin-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir).await
            .map_err(|e| PluginError::installation(format!("Failed to create temp directory: {}", e)))?;

        // Download file
        let response = reqwest::get(url).await
            .map_err(|e| PluginError::installation(format!("Failed to download plugin: {}", e)))?;

        let file_path = temp_dir.join("plugin.zip");
        let mut file = tokio::fs::File::create(&file_path).await
            .map_err(|e| PluginError::installation(format!("Failed to create plugin file: {}", e)))?;

        let bytes = response.bytes().await
            .map_err(|e| PluginError::installation(format!("Failed to read plugin bytes: {}", e)))?;

        use tokio::io::AsyncWriteExt;
        file.write_all(&bytes).await
            .map_err(|e| PluginError::installation(format!("Failed to write plugin file: {}", e)))?;

        // Extract plugin
        let extract_dir = temp_dir.join("extracted");
        self.extract_plugin(&file_path, &extract_dir).await?;

        // Install from extracted directory
        let result = self.install_from_directory(&extract_dir).await;

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;

        result
    }

    /// Install plugin
    async fn install_plugin(&self, mut manifest: PluginManifest) -> PluginResult<String> {
        // Validate plugin
        let registry = self.registry.read().await;
        let validation_results = registry.validate_plugin(&manifest).await?;
        drop(registry);

        if !validation_results.valid && self.config.auto_discovery.validation.strict {
            return Err(PluginError::installation(format!(
                "Plugin validation failed: {} issues found",
                validation_results.security_validation.issues.len()
            )));
        }

        // Install to plugin directory
        let install_dir = self.config.plugin_directories.first()
            .ok_or_else(|| PluginError::installation("No plugin directory configured".to_string()))?;

        let plugin_dir = install_dir.join(&manifest.id);
        tokio::fs::create_dir_all(&plugin_dir).await
            .map_err(|e| PluginError::installation(format!("Failed to create plugin directory: {}", e)))?;

        // Copy plugin files
        self.copy_plugin_files(&manifest.entry_point, &plugin_dir).await?;

        // Update manifest entry point to installed location
        let entry_file_name = manifest.entry_point.file_name()
            .ok_or_else(|| PluginError::installation("Invalid entry point".to_string()))?;
        manifest.entry_point = plugin_dir.join(entry_file_name);

        // Save manifest
        let manifest_path = plugin_dir.join("plugin.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| PluginError::installation(format!("Failed to serialize manifest: {}", e)))?;

        tokio::fs::write(&manifest_path, manifest_json).await
            .map_err(|e| PluginError::installation(format!("Failed to write manifest: {}", e)))?;

        // Register plugin
        let mut registry = self.registry.write().await;
        registry.register_plugin(manifest).await
    }

    /// Load manifest from file
    async fn load_manifest(&self, path: &Path) -> PluginResult<PluginManifest> {
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| PluginError::installation(format!("Failed to read manifest: {}", e)))?;

        serde_json::from_str(&content)
            .map_err(|e| PluginError::installation(format!("Failed to parse manifest: {}", e)))
    }

    /// Copy plugin files to installation directory
    async fn copy_plugin_files(&self, entry_point: &Path, install_dir: &Path) -> PluginResult<()> {
        if entry_point.is_file() {
            let file_name = entry_point.file_name()
                .ok_or_else(|| PluginError::installation("Invalid entry point".to_string()))?;
            let dest_path = install_dir.join(file_name);

            tokio::fs::copy(entry_point, &dest_path).await
                .map_err(|e| PluginError::installation(format!("Failed to copy plugin file: {}", e)))?;
        } else if entry_point.is_dir() {
            // Copy entire directory
            self.copy_dir_all(entry_point, install_dir).await?;
        }

        Ok(())
    }

    /// Copy directory recursively
    async fn copy_dir_all(&self, src: &Path, dst: &Path) -> PluginResult<()> {
        tokio::fs::create_dir_all(dst).await
            .map_err(|e| PluginError::installation(format!("Failed to create directory: {}", e)))?;

        let mut entries = tokio::fs::read_dir(src).await
            .map_err(|e| PluginError::installation(format!("Failed to read directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| PluginError::installation(format!("Failed to read directory entry: {}", e)))? {

            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                self.copy_dir_all(&src_path, &dst_path).await?;
            } else {
                tokio::fs::copy(&src_path, &dst_path).await
                    .map_err(|e| PluginError::installation(format!("Failed to copy file: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Extract plugin archive
    async fn extract_plugin(&self, archive_path: &Path, extract_dir: &Path) -> PluginResult<()> {
        // This is a simplified implementation
        // In a real implementation, you'd use a proper archive extraction library

        #[cfg(unix)]
        {
            use tokio::process::Command;

            let output = Command::new("unzip")
                .arg("-q")
                .arg(archive_path)
                .arg("-d")
                .arg(extract_dir)
                .output()
                .await
                .map_err(|e| PluginError::installation(format!("Failed to extract plugin: {}", e)))?;

            if !output.status.success() {
                return Err(PluginError::installation(format!(
                    "Failed to extract plugin: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }

        #[cfg(not(unix))]
        {
            return Err(PluginError::installation(
                "Plugin extraction not supported on this platform".to_string()
            ));
        }

        Ok(())
    }
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

/// Create a default plugin registry with configuration
pub fn create_plugin_registry(config: PluginManagerConfig) -> Box<dyn PluginRegistry> {
    Box::new(DefaultPluginRegistry::new(config))
}