//! Profile configuration for environment-specific settings.

use crate::{ConfigError, DatabaseConfig, EnrichmentConfig, LoggingConfig, ServerConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for different environments/profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Profile name.
    pub name: String,

    /// Profile description.
    pub description: Option<String>,

    /// Environment type.
    pub environment: Environment,

    /// Enrichment configuration (includes embedding provider) for this profile.
    pub enrichment: Option<EnrichmentConfig>,

    /// Database configuration for this profile.
    pub database: Option<DatabaseConfig>,

    /// Server configuration for this profile.
    pub server: Option<ServerConfig>,

    /// Logging configuration for this profile.
    pub logging: Option<LoggingConfig>,

    /// Environment variables for this profile.
    #[serde(default)]
    pub env_vars: HashMap<String, String>,

    /// Profile-specific settings.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub settings: HashMap<String, serde_json::Value>,
}

impl ProfileConfig {
    /// Create a new profile configuration.
    pub fn new(name: String, environment: Environment) -> Self {
        Self {
            name,
            description: None,
            environment,
            enrichment: None,
            database: None,
            server: None,
            logging: None,
            env_vars: HashMap::new(),
            settings: HashMap::new(),
        }
    }

    /// Create a development profile.
    pub fn development() -> Self {
        let mut profile = Self::new("development".to_string(), Environment::Development);
        profile.description = Some("Development environment with debug logging".to_string());
        profile.logging = Some(LoggingConfig {
            level: "debug".to_string(),
            format: "text".to_string(),
            file: false,
            file_path: None,
            max_file_size: Some(10 * 1024 * 1024),
            max_files: Some(3),
            ..Default::default()
        });
        profile
    }

    /// Create a production profile.
    pub fn production() -> Self {
        let mut profile = Self::new("production".to_string(), Environment::Production);
        profile.description = Some("Production environment with optimized settings".to_string());
        profile.logging = Some(LoggingConfig {
            level: "warn".to_string(),
            format: "json".to_string(),
            file: true,
            file_path: Some("/var/log/crucible/app.log".to_string()),
            max_file_size: Some(100 * 1024 * 1024),
            max_files: Some(10),
            ..Default::default()
        });
        profile
    }

    /// Create a testing profile.
    pub fn testing() -> Self {
        let mut profile = Self::new("testing".to_string(), Environment::Test);
        profile.description = Some("Testing environment with in-memory storage".to_string());
        profile.logging = Some(LoggingConfig {
            level: "error".to_string(),
            format: "text".to_string(),
            file: false,
            file_path: None,
            max_file_size: None,
            max_files: None,
            ..Default::default()
        });
        profile
    }

    /// Add an environment variable to the profile.
    pub fn env_var(mut self, key: String, value: String) -> Self {
        self.env_vars.insert(key, value);
        self
    }

    /// Add a custom setting to the profile.
    pub fn setting<T>(mut self, key: String, value: T) -> Result<Self, ConfigError>
    where
        T: serde::Serialize,
    {
        let json_value = serde_json::to_value(value).map_err(ConfigError::Serialization)?;
        self.settings.insert(key, json_value);
        Ok(self)
    }

    /// Get an environment variable value.
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.env_vars.get(key)
    }

    /// Get a custom setting value.
    pub fn get_setting<T>(&self, key: &str) -> Result<Option<T>, ConfigError>
    where
        T: for<'de> Deserialize<'de>,
    {
        if let Some(value) = self.settings.get(key) {
            let typed = serde_json::from_value(value.clone())?;
            Ok(Some(typed))
        } else {
            Ok(None)
        }
    }

    /// Check if this profile is for the given environment.
    pub fn is_environment(&self, env: Environment) -> bool {
        self.environment == env
    }

    /// Merge this profile with another profile, with the other profile taking precedence.
    pub fn merge_with(self, other: ProfileConfig) -> ProfileConfig {
        let mut merged = self;

        // Override description if present in other
        if other.description.is_some() {
            merged.description = other.description;
        }

        // Override environment if present in other
        if other.environment != Environment::Development {
            merged.environment = other.environment;
        }

        // Override configurations if present in other
        if other.enrichment.is_some() {
            merged.enrichment = other.enrichment;
        }
        if other.database.is_some() {
            merged.database = other.database;
        }
        if other.server.is_some() {
            merged.server = other.server;
        }
        if other.logging.is_some() {
            merged.logging = other.logging;
        }

        // Merge environment variables (other takes precedence)
        for (key, value) in other.env_vars {
            merged.env_vars.insert(key, value);
        }

        // Merge settings (other takes precedence)
        for (key, value) in other.settings {
            merged.settings.insert(key, value);
        }

        merged
    }
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self::development()
    }
}

/// Environment types for profiles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Development environment.
    #[default]
    Development,
    /// Testing environment.
    Test,
    /// Staging environment.
    Staging,
    /// Production environment.
    Production,
}

impl Environment {
    /// Get the string representation of the environment.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    /// Parse environment from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "development" | "dev" | "debug" => Some(Self::Development),
            "test" | "testing" => Some(Self::Test),
            "staging" | "stage" => Some(Self::Staging),
            "production" | "prod" | "release" => Some(Self::Production),
            _ => None,
        }
    }

    /// Check if this is a production environment.
    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }

    /// Check if this is a development environment.
    pub fn is_development(&self) -> bool {
        matches!(self, Self::Development)
    }

    /// Check if this is a testing environment.
    pub fn is_test(&self) -> bool {
        matches!(self, Self::Test)
    }

    /// Check if debug features should be enabled.
    pub fn enable_debug(&self) -> bool {
        !matches!(self, Self::Production)
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Profile manager for handling multiple profiles.
#[derive(Debug, Clone)]
pub struct ProfileManager {
    profiles: HashMap<String, ProfileConfig>,
    default_profile: String,
}

impl ProfileManager {
    /// Create a new profile manager.
    pub fn new() -> Self {
        let mut manager = Self {
            profiles: HashMap::new(),
            default_profile: "default".to_string(),
        };

        // Add default profiles
        manager.add_profile(ProfileConfig::development());
        manager.add_profile(ProfileConfig::testing());
        manager.add_profile(ProfileConfig::production());

        manager
    }

    /// Add a profile to the manager.
    pub fn add_profile(&mut self, profile: ProfileConfig) {
        let name = profile.name.clone();
        self.profiles.insert(name, profile);
    }

    /// Get a profile by name.
    pub fn get_profile(&self, name: &str) -> Option<&ProfileConfig> {
        self.profiles.get(name)
    }

    /// Get the default profile.
    pub fn get_default_profile(&self) -> &ProfileConfig {
        self.get_profile(&self.default_profile)
            .or_else(|| self.get_profile("development"))
            .expect("no default profile and no 'development' profile found; ensure at least one profile exists in config")
    }

    /// Set the default profile.
    pub fn set_default_profile(&mut self, name: String) -> Result<(), ConfigError> {
        if !self.profiles.contains_key(&name) {
            return Err(ConfigError::MissingValue {
                field: format!("profile.{}", name),
            });
        }
        self.default_profile = name;
        Ok(())
    }

    /// List all available profiles.
    pub fn list_profiles(&self) -> Vec<&String> {
        self.profiles.keys().collect()
    }

    /// Get profiles by environment.
    pub fn get_profiles_by_environment(&self, env: Environment) -> Vec<&ProfileConfig> {
        self.profiles
            .values()
            .filter(|profile| profile.is_environment(env))
            .collect()
    }

    /// Merge profiles (base profile gets overridden by overlay profile).
    pub fn merge_profiles(&self, base: &str, overlay: &str) -> Option<ProfileConfig> {
        let base_profile = self.get_profile(base)?;
        let overlay_profile = self.get_profile(overlay)?;
        Some(base_profile.clone().merge_with(overlay_profile.clone()))
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}
