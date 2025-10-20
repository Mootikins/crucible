//! State management system for Crucible
//!
//! This module provides centralized application state management, event handling,
//! persistence, and recovery mechanisms for the entire application.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::ConfigManager;

/// Application state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationState {
    /// Global application metadata
    pub metadata: StateMetadata,
    /// User preferences and settings
    pub user_preferences: UserPreferences,
    /// System configuration overrides
    pub system_overrides: HashMap<String, serde_json::Value>,
    /// Feature flags
    pub feature_flags: HashMap<String, bool>,
    /// Runtime statistics
    pub runtime_stats: RuntimeStats,
    /// Cache state
    pub cache_state: CacheState,
}

impl Default for ApplicationState {
    fn default() -> Self {
        Self {
            metadata: StateMetadata::default(),
            user_preferences: UserPreferences::default(),
            system_overrides: HashMap::new(),
            feature_flags: HashMap::new(),
            runtime_stats: RuntimeStats::default(),
            cache_state: CacheState::default(),
        }
    }
}

/// State metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMetadata {
    /// State version
    pub version: String,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last modification timestamp
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// State identifier
    pub state_id: uuid::Uuid,
    /// Previous state identifier
    pub previous_state_id: Option<uuid::Uuid>,
    /// Checksum for integrity verification
    pub checksum: Option<String>,
}

impl Default for StateMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            version: "1.0.0".to_string(),
            created_at: now,
            modified_at: now,
            state_id: uuid::Uuid::new_v4(),
            previous_state_id: None,
            checksum: None,
        }
    }
}

/// User preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Theme preference
    pub theme: String,
    /// Language preference
    pub language: String,
    /// Timezone
    pub timezone: String,
    /// Notification settings
    pub notifications: NotificationSettings,
    /// Editor settings
    pub editor: EditorSettings,
    /// UI preferences
    pub ui: UIPreferences,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            language: "en".to_string(),
            timezone: "UTC".to_string(),
            notifications: NotificationSettings::default(),
            editor: EditorSettings::default(),
            ui: UIPreferences::default(),
        }
    }
}

/// Notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    /// Enable desktop notifications
    pub desktop_enabled: bool,
    /// Enable email notifications
    pub email_enabled: bool,
    /// Notification levels
    pub levels: HashMap<String, bool>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        let mut levels = HashMap::new();
        levels.insert("info".to_string(), true);
        levels.insert("warning".to_string(), true);
        levels.insert("error".to_string(), true);
        levels.insert("critical".to_string(), true);

        Self {
            desktop_enabled: true,
            email_enabled: false,
            levels,
        }
    }
}

/// Editor settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSettings {
    /// Font size
    pub font_size: u32,
    /// Font family
    pub font_family: String,
    /// Tab size
    pub tab_size: u32,
    /// Enable word wrap
    pub word_wrap: bool,
    /// Enable line numbers
    pub line_numbers: bool,
    /// Syntax highlighting theme
    pub syntax_theme: String,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_size: 14,
            font_family: "monospace".to_string(),
            tab_size: 4,
            word_wrap: true,
            line_numbers: true,
            syntax_theme: "default".to_string(),
        }
    }
}

/// UI preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIPreferences {
    /// Sidebar width
    pub sidebar_width: u32,
    /// Panel layout
    pub panel_layout: String,
    /// Show/hide panels
    pub panels: HashMap<String, bool>,
    /// Window state
    pub window_state: Option<WindowState>,
}

impl Default for UIPreferences {
    fn default() -> Self {
        let mut panels = HashMap::new();
        panels.insert("sidebar".to_string(), true);
        panels.insert("status_bar".to_string(), true);
        panels.insert("toolbar".to_string(), true);

        Self {
            sidebar_width: 300,
            panel_layout: "default".to_string(),
            panels,
            window_state: None,
        }
    }
}

/// Window state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// Runtime statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStats {
    /// Application start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Total uptime in seconds
    pub uptime_seconds: u64,
    /// Number of user sessions
    pub session_count: u64,
    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
    /// Performance metrics
    pub performance: PerformanceMetrics,
}

impl Default for RuntimeStats {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            start_time: now,
            uptime_seconds: 0,
            session_count: 1,
            last_activity: now,
            performance: PerformanceMetrics::default(),
        }
    }
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Disk usage in bytes
    pub disk_usage_bytes: u64,
    /// Network usage in bytes
    pub network_usage_bytes: u64,
    /// Request count
    pub request_count: u64,
    /// Error count
    pub error_count: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
            disk_usage_bytes: 0,
            network_usage_bytes: 0,
            request_count: 0,
            error_count: 0,
        }
    }
}

/// Cache state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheState {
    /// Cache entries
    pub entries: HashMap<String, CacheEntry>,
    /// Cache statistics
    pub stats: CacheStats,
}

impl Default for CacheState {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            stats: CacheStats::default(),
        }
    }
}

/// Cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Cache key
    pub key: String,
    /// Cached value
    pub value: serde_json::Value,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Expiration timestamp
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Access count
    pub access_count: u64,
    /// Last access timestamp
    pub last_accessed: chrono::DateTime<chrono::Utc>,
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total cache size in bytes
    pub total_size_bytes: u64,
    /// Hit count
    pub hit_count: u64,
    /// Miss count
    pub miss_count: u64,
    /// Eviction count
    pub eviction_count: u64,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_size_bytes: 0,
            hit_count: 0,
            miss_count: 0,
            eviction_count: 0,
        }
    }
}

/// State management command
#[derive(Debug)]
pub enum StateCommand {
    GetState {
        key: Option<String>,
        response_tx: oneshot::Sender<Result<serde_json::Value>>,
    },
    SetState {
        key: String,
        value: serde_json::Value,
        response_tx: oneshot::Sender<Result<()>>,
    },
    DeleteState {
        key: String,
        response_tx: oneshot::Sender<Result<bool>>,
    },
    GetApplicationState {
        response_tx: oneshot::Sender<Result<ApplicationState>>,
    },
    SetApplicationState {
        state: ApplicationState,
        response_tx: oneshot::Sender<Result<()>>,
    },
    PersistState {
        response_tx: oneshot::Sender<Result<()>>,
    },
    LoadState {
        response_tx: oneshot::Sender<Result<ApplicationState>>,
    },
    GetCache {
        key: String,
        response_tx: oneshot::Sender<Result<Option<serde_json::Value>>>,
    },
    SetCache {
        key: String,
        value: serde_json::Value,
        ttl: Option<Duration>,
        response_tx: oneshot::Sender<Result<()>>,
    },
    ClearCache {
        response_tx: oneshot::Sender<Result<()>>,
    },
}

/// State change event
#[derive(Debug, Clone)]
pub enum StateEvent {
    Changed {
        key: String,
        old_value: Option<serde_json::Value>,
        new_value: serde_json::Value,
    },
    Deleted {
        key: String,
        value: serde_json::Value,
    },
    Persisted {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Loaded {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    CacheHit {
        key: String,
    },
    CacheMiss {
        key: String,
    },
}

/// State manager
pub struct StateManager {
    /// Configuration manager
    config_manager: Arc<ConfigManager>,
    /// Application state
    application_state: Arc<RwLock<ApplicationState>>,
    /// Custom state storage
    custom_state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    /// Command receiver
    command_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<StateCommand>>>,
    /// Command sender
    command_tx: mpsc::UnboundedSender<StateCommand>,
    /// Event broadcaster
    event_tx: broadcast::Sender<StateEvent>,
    /// State persistence
    persistence: Arc<StatePersistence>,
    /// Running state
    running: Arc<RwLock<bool>>,
}

/// State persistence handler
#[derive(Debug)]
struct StatePersistence {
    config_manager: Arc<ConfigManager>,
    persistence_path: Option<std::path::PathBuf>,
}

impl StatePersistence {
    /// Create new persistence handler
    fn new(config_manager: Arc<ConfigManager>) -> Self {
        let persistence_path = std::env::var("CRUCIBLE_STATE_PATH")
            .ok()
            .or_else(|| Some("crucible_state.json".to_string()))
            .map(std::path::PathBuf::from);

        Self {
            config_manager,
            persistence_path,
        }
    }

    /// Persist state to disk
    async fn persist(&self, state: &ApplicationState) -> Result<()> {
        if let Some(path) = &self.persistence_path {
            let json = serde_json::to_string_pretty(state)
                .context("Failed to serialize state")?;

            // Create directory if it doesn't exist
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await
                    .context("Failed to create state directory")?;
            }

            // Write state atomically
            let temp_path = path.with_extension("tmp");
            tokio::fs::write(&temp_path, json).await
                .context("Failed to write temporary state file")?;

            tokio::fs::rename(&temp_path, path).await
                .context("Failed to rename state file")?;

            info!("State persisted to: {:?}", path);
        }

        Ok(())
    }

    /// Load state from disk
    async fn load(&self) -> Result<ApplicationState> {
        if let Some(path) = &self.persistence_path {
            if path.exists() {
                let content = tokio::fs::read_to_string(path).await
                    .context("Failed to read state file")?;

                let state: ApplicationState = serde_json::from_str(&content)
                    .context("Failed to deserialize state")?;

                info!("State loaded from: {:?}", path);
                return Ok(state);
            }
        }

        // Return default state if no persistence file exists
        Ok(ApplicationState::default())
    }
}

impl std::fmt::Debug for StateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManager")
            .field("custom_state_count", &self.custom_state.try_read().map(|s| s.len()).unwrap_or(0))
            .field("running", &self.running)
            .finish()
    }
}

impl StateManager {
    /// Create a new state manager
    pub async fn new(config_manager: Arc<ConfigManager>) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(1000);

        let persistence = Arc::new(StatePersistence::new(config_manager.clone()));

        let state_manager = Self {
            config_manager,
            application_state: Arc::new(RwLock::new(ApplicationState::default())),
            custom_state: Arc::new(RwLock::new(HashMap::new())),
            command_rx: Arc::new(tokio::sync::Mutex::new(command_rx)),
            command_tx,
            event_tx,
            persistence,
            running: Arc::new(RwLock::new(false)),
        };

        // Initialize with persisted state if available
        state_manager.initialize_from_persistence().await?;

        Ok(state_manager)
    }

    /// Initialize from persisted state
    async fn initialize_from_persistence(&self) -> Result<()> {
        match self.persistence.load().await {
            Ok(persisted_state) => {
                *self.application_state.write().await = persisted_state;
                info!("State manager initialized with persisted state");
            }
            Err(e) => {
                warn!("Failed to load persisted state, using defaults: {}", e);
            }
        }

        Ok(())
    }

    /// Get command sender
    pub fn command_sender(&self) -> mpsc::UnboundedSender<StateCommand> {
        self.command_tx.clone()
    }

    /// Subscribe to state events
    pub fn subscribe_events(&self) -> broadcast::Receiver<StateEvent> {
        self.event_tx.subscribe()
    }

    /// Start the state manager
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("State manager is already running");
            return Ok(());
        }

        *running = true;
        info!("State manager started");

        // Start command processing
        self.start_command_processing_task();

        // Start periodic persistence
        self.start_persistence_task();

        // Start runtime stats updates
        self.start_runtime_stats_task();

        // Start cache cleanup
        self.start_cache_cleanup_task();

        Ok(())
    }

    /// Stop the state manager
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            warn!("State manager is not running");
            return Ok(());
        }

        // Persist final state
        if let Err(e) = self.persist_state().await {
            error!("Failed to persist state during shutdown: {}", e);
        }

        *running = false;
        info!("State manager stopped");

        Ok(())
    }

    /// Get custom state value
    pub async fn get_state(&self, key: &str) -> Option<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        let _ = self.command_tx.send(StateCommand::GetState {
            key: Some(key.to_string()),
            response_tx: tx,
        });

        rx.await.ok()?.ok()
    }

    /// Set custom state value
    pub async fn set_state(&self, key: String, value: serde_json::Value) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(StateCommand::SetState {
            key,
            value,
            response_tx: tx,
        })?;

        Ok(rx.await??)
    }

    /// Delete custom state value
    pub async fn delete_state(&self, key: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(StateCommand::DeleteState {
            key: key.to_string(),
            response_tx: tx,
        })?;

        Ok(rx.await??)
    }

    /// Get application state
    pub async fn get_application_state(&self) -> ApplicationState {
        let (tx, rx) = oneshot::channel();
        let _ = self.command_tx.send(StateCommand::GetApplicationState { response_tx: tx });
        rx.await.unwrap_or_else(|_| Ok(ApplicationState::default())).unwrap_or_default()
    }

    /// Set application state
    pub async fn set_application_state(&self, state: ApplicationState) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(StateCommand::SetApplicationState {
            state,
            response_tx: tx,
        })?;

        Ok(rx.await??)
    }

    /// Get cache value
    pub async fn get_cache(&self, key: &str) -> Option<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        let _ = self.command_tx.send(StateCommand::GetCache {
            key: key.to_string(),
            response_tx: tx,
        });

        match rx.await {
            Ok(Ok(value)) => value,
            Ok(Err(_)) | Err(_) => None,
        }
    }

    /// Set cache value
    pub async fn set_cache(&self, key: String, value: serde_json::Value, ttl: Option<Duration>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(StateCommand::SetCache {
            key,
            value,
            ttl,
            response_tx: tx,
        })?;

        Ok(rx.await??)
    }

    /// Perform health check
    pub async fn health_check(&self) -> Result<bool> {
        let running = *self.running.read().await;
        if !running {
            return Ok(false);
        }

        // Check if we can access state
        let _ = self.application_state.read().await;

        // Check persistence functionality
        if let Err(e) = self.persistence.persist(&*self.application_state.read().await).await {
            warn!("State persistence health check failed: {}", e);
            // Don't fail health check for persistence issues
        }

        Ok(true)
    }

    /// Start command processing task
    fn start_command_processing_task(&self) {
        let application_state = self.application_state.clone();
        let custom_state = self.custom_state.clone();
        let event_tx = self.event_tx.clone();
        let persistence = self.persistence.clone();
        let running = self.running.clone();
        let command_rx = self.command_rx.clone();

        tokio::spawn(async move {
            while *running.read().await {
                let command = {
                    let mut rx = command_rx.lock().await;
                    rx.recv().await
                };

                match command {
                    Some(command) => {
                        Self::handle_command(
                            command,
                            application_state.clone(),
                            custom_state.clone(),
                            event_tx.clone(),
                            persistence.clone(),
                        ).await;
                    }
                    None => break,
                }
            }
        });
    }

    /// Handle state management command
    async fn handle_command(
        command: StateCommand,
        application_state: Arc<RwLock<ApplicationState>>,
        custom_state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
        event_tx: broadcast::Sender<StateEvent>,
        persistence: Arc<StatePersistence>,
    ) {
        match command {
            StateCommand::GetState { key, response_tx } => {
                let result = if let Some(key) = key {
                    custom_state.read().await.get(&key).cloned()
                } else {
                    Some(serde_json::to_value(&*custom_state.read().await).unwrap_or_default())
                };

                let _ = response_tx.send(Ok(result.unwrap_or(serde_json::Value::Null)));
            }
            StateCommand::SetState { key, value, response_tx } => {
                let old_value = custom_state.read().await.get(&key).cloned();
                custom_state.write().await.insert(key.clone(), value.clone());

                // Send event
                let event = StateEvent::Changed {
                    key,
                    old_value,
                    new_value: value,
                };
                let _ = event_tx.send(event);

                let _ = response_tx.send(Ok(()));
            }
            StateCommand::DeleteState { key, response_tx } => {
                let result = custom_state.write().await.remove(&key);
                let deleted = result.is_some();

                if let Some(value) = result {
                    // Send event
                    let event = StateEvent::Deleted { key, value };
                    let _ = event_tx.send(event);
                }

                let _ = response_tx.send(Ok(deleted));
            }
            StateCommand::GetApplicationState { response_tx } => {
                let state = application_state.read().await.clone();
                let _ = response_tx.send(Ok(state));
            }
            StateCommand::SetApplicationState { state, response_tx } => {
                let mut app_state = application_state.write().await;
                let old_state_id = app_state.metadata.state_id;

                app_state.metadata.previous_state_id = Some(old_state_id);
                app_state.metadata.modified_at = chrono::Utc::now();
                app_state.metadata.state_id = uuid::Uuid::new_v4();

                *app_state = state;
                let _ = response_tx.send(Ok(()));
            }
            StateCommand::PersistState { response_tx } => {
                let result = persistence.persist(&*application_state.read().await).await;

                if result.is_ok() {
                    let event = StateEvent::Persisted {
                        timestamp: chrono::Utc::now(),
                    };
                    let _ = event_tx.send(event);
                }

                let _ = response_tx.send(result);
            }
            StateCommand::LoadState { response_tx } => {
                let result = persistence.load().await;

                if let Ok(state) = &result {
                    *application_state.write().await = state.clone();

                    let event = StateEvent::Loaded {
                        timestamp: chrono::Utc::now(),
                    };
                    let _ = event_tx.send(event);
                }

                let _ = response_tx.send(result);
            }
            StateCommand::GetCache { key, response_tx } => {
                let app_state = application_state.read().await;
                let should_use = app_state.cache_state.entries.get(&key)
                    .filter(|entry| {
                        // Check expiration
                        entry.expires_at.map_or(true, |expires| expires > chrono::Utc::now())
                    })
                    .is_some();

        let result = if should_use {
            // Update access stats
            drop(app_state);
            let app_state = application_state.write().await;
            app_state.cache_state.entries.get(&key).map(|entry| {
                entry.value.clone()
            })
        } else {
            None
        };

                // Send cache event
                if result.is_some() {
                    let event = StateEvent::CacheHit { key };
                    let _ = event_tx.send(event);
                } else {
                    let event = StateEvent::CacheMiss { key };
                    let _ = event_tx.send(event);
                }

                let _ = response_tx.send(Ok(result));
            }
            StateCommand::SetCache { key, value, ttl, response_tx } => {
                let now = chrono::Utc::now();
                let expires_at = ttl.map(|duration| now + chrono::Duration::from_std(duration).unwrap());

                let mut app_state = application_state.write().await;
                let entry = CacheEntry {
                    key: key.clone(),
                    value: value.clone(),
                    created_at: now,
                    expires_at,
                    access_count: 0,
                    last_accessed: now,
                };

                app_state.cache_state.entries.insert(key.clone(), entry);

                let _ = response_tx.send(Ok(()));
            }
            StateCommand::ClearCache { response_tx } => {
                application_state.write().await.cache_state.entries.clear();
                let _ = response_tx.send(Ok(()));
            }
        }
    }

    /// Start periodic persistence task
    fn start_persistence_task(&self) {
        let application_state = self.application_state.clone();
        let persistence = self.persistence.clone();
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Persist every 5 minutes

            while *running.read().await {
                interval.tick().await;

                if let Err(e) = persistence.persist(&*application_state.read().await).await {
                    error!("Failed to persist state: {}", e);
                } else {
                    let event = StateEvent::Persisted {
                        timestamp: chrono::Utc::now(),
                    };
                    let _ = event_tx.send(event);
                }
            }
        });
    }

    /// Start runtime stats update task
    fn start_runtime_stats_task(&self) {
        let application_state = self.application_state.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Update every minute

            while *running.read().await {
                interval.tick().await;

                let mut app_state = application_state.write().await;
                app_state.runtime_stats.uptime_seconds =
                    (chrono::Utc::now() - app_state.runtime_stats.start_time).num_seconds() as u64;
                app_state.runtime_stats.last_activity = chrono::Utc::now();
            }
        });
    }

    /// Start cache cleanup task
    fn start_cache_cleanup_task(&self) {
        let application_state = self.application_state.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600)); // Clean every 10 minutes

            while *running.read().await {
                interval.tick().await;

                let now = chrono::Utc::now();
                let mut app_state = application_state.write().await;

                // Remove expired entries
                let initial_count = app_state.cache_state.entries.len();
                app_state.cache_state.entries.retain(|_, entry| {
                    entry.expires_at.map_or(true, |expires| expires > now)
                });

                let removed_count = initial_count - app_state.cache_state.entries.len();
                if removed_count > 0 {
                    debug!("Removed {} expired cache entries", removed_count);
                }

                // Update cache stats
                app_state.cache_state.stats.eviction_count += removed_count as u64;
            }
        });
    }

    /// Persist state immediately
    async fn persist_state(&self) -> Result<()> {
        self.persistence.persist(&*self.application_state.read().await).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_state_manager_creation() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let state_manager = StateManager::new(config_manager).await;

        assert!(state_manager.is_ok());
    }

    #[tokio::test]
    async fn test_custom_state_operations() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let state_manager = StateManager::new(config_manager).await.unwrap();

        // Set state
        state_manager.set_state(
            "test_key".to_string(),
            serde_json::json!("test_value"),
        ).await.unwrap();

        // Get state
        let value = state_manager.get_state("test_key").await;
        assert_eq!(value, Some(serde_json::json!("test_value")));

        // Delete state
        let deleted = state_manager.delete_state("test_key").await.unwrap();
        assert!(deleted);

        // Verify deletion
        let value = state_manager.get_state("test_key").await;
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let state_manager = StateManager::new(config_manager).await.unwrap();

        // Set cache
        state_manager.set_cache(
            "cache_key".to_string(),
            serde_json::json!("cache_value"),
            Some(Duration::from_secs(60)),
        ).await.unwrap();

        // Get cache
        let value = state_manager.get_cache("cache_key").await;
        assert_eq!(value, Some(serde_json::json!("cache_value")));

        // Clear cache
        let (tx, rx) = oneshot::channel();
        state_manager.command_tx.send(StateCommand::ClearCache { response_tx: tx }).unwrap();
        rx.await.unwrap().unwrap();

        // Verify cache cleared
        let value = state_manager.get_cache("cache_key").await;
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_application_state() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let state_manager = StateManager::new(config_manager).await.unwrap();

        let state = state_manager.get_application_state().await;
        assert_eq!(state.user_preferences.theme, "light");

        // Modify state
        let mut new_state = state;
        new_state.user_preferences.theme = "dark".to_string();

        state_manager.set_application_state(new_state).await.unwrap();

        // Verify change
        let updated_state = state_manager.get_application_state().await;
        assert_eq!(updated_state.user_preferences.theme, "dark");
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let state_manager = StateManager::new(config_manager).await.unwrap();
        let mut events = state_manager.subscribe_events();

        // Set state
        state_manager.set_state(
            "test_key".to_string(),
            serde_json::json!("test_value"),
        ).await.unwrap();

        // Should receive state changed event
        let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            StateEvent::Changed { key, new_value, .. } => {
                assert_eq!(key, "test_key");
                assert_eq!(new_value, serde_json::json!("test_value"));
            }
            _ => panic!("Expected state changed event"),
        }
    }

    #[tokio::test]
    async fn test_state_manager_lifecycle() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let state_manager = StateManager::new(config_manager).await.unwrap();

        // Start state manager
        state_manager.start().await.unwrap();
        assert_eq!(*state_manager.running.read().await, true);

        // Health check
        let healthy = state_manager.health_check().await.unwrap();
        assert!(healthy);

        // Stop state manager
        state_manager.stop().await.unwrap();
        assert_eq!(*state_manager.running.read().await, false);
    }
}