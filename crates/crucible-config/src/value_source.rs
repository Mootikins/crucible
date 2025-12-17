//! Value source tracking for configuration
//!
//! This module provides functionality to track where each configuration
//! value was set (file, environment, CLI, or default).

use std::collections::HashMap;

/// Source of a configuration value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    /// Value came from the configuration file
    File { path: Option<String> },
    /// Value came from an environment variable
    Environment { var: String },
    /// Value came from a CLI argument
    Cli,
    /// Value is the system default
    Default,
    /// Value was overridden by a profile
    Profile { name: String },
    /// Value was included from another file
    Included { file: String },
}

impl ValueSource {
    /// Get a short description of the source
    pub fn short(&self) -> &'static str {
        match self {
            ValueSource::File { .. } => "file",
            ValueSource::Environment { .. } => "env",
            ValueSource::Cli => "cli",
            ValueSource::Default => "default",
            ValueSource::Profile { .. } => "profile",
            ValueSource::Included { .. } => "included",
        }
    }

    /// Get a detailed description of the source
    pub fn detail(&self) -> String {
        match self {
            ValueSource::File { path } => {
                if let Some(p) = path {
                    format!("file ({})", p)
                } else {
                    "file".to_string()
                }
            }
            ValueSource::Environment { var } => format!("environment ({})", var),
            ValueSource::Cli => "cli argument".to_string(),
            ValueSource::Default => "default".to_string(),
            ValueSource::Profile { name } => format!("profile ({})", name),
            ValueSource::Included { file } => format!("included ({})", file),
        }
    }
}

/// A configuration value with its source
#[derive(Debug, Clone)]
pub struct ValueWithSource<T> {
    /// The actual value
    pub value: T,
    /// Where the value came from
    pub source: ValueSource,
}

impl<T> ValueWithSource<T> {
    /// Create a new value with source
    pub fn new(value: T, source: ValueSource) -> Self {
        Self { value, source }
    }
}

/// A map of configuration values with their sources
#[derive(Debug, Clone, Default)]
pub struct ValueSourceMap {
    /// Map of value paths to their sources
    values: HashMap<String, ValueSource>,
}

impl ValueSourceMap {
    /// Create a new empty map
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a value source
    pub fn set(&mut self, path: &str, source: ValueSource) {
        self.values.insert(path.to_string(), source);
    }

    /// Get the source of a value
    pub fn get(&self, path: &str) -> Option<&ValueSource> {
        self.values.get(path)
    }

    /// Check if a value has a specific source
    pub fn has_source(&self, path: &str, source: &ValueSource) -> bool {
        self.values
            .get(path)
            .map(|s| s == source)
            .unwrap_or(false)
    }

    /// Get all values from a specific source
    pub fn values_from_source(&self, source: &ValueSource) -> Vec<&str> {
        self.values
            .iter()
            .filter(|(_, s)| *s == source)
            .map(|(k, _)| k.as_str())
            .collect()
    }

    /// Convert to a serializable map (for JSON output with sources)
    pub fn to_serializable<T>(&self, get_value: impl Fn(&str) -> Option<T>) -> HashMap<String, ValueInfo<T>> {
        self.values
            .iter()
            .filter_map(|(path, source)| {
                get_value(path).map(|value| {
                    (
                        path.clone(),
                        ValueInfo {
                            value,
                            source: source.detail(),
                            source_short: source.short().to_string(),
                        },
                    )
                })
            })
            .collect()
    }
}

/// Information about a value for serialization
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValueInfo<T> {
    /// The actual value
    pub value: T,
    /// Detailed source description
    pub source: String,
    /// Short source description
    pub source_short: String,
}

/// Trait for tracking value sources during config loading
pub trait TrackValueSource {
    /// Set the source map for tracking
    fn set_source_map(&mut self, source_map: ValueSourceMap);

    /// Get the source map
    fn get_source_map(&self) -> &ValueSourceMap;
}

/// Macro to help with tracking value sources during config loading
#[macro_export]
macro_rules! track_value {
    ($config:expr, $path:expr, $value:expr, $source:expr) => {
        if let Some(ref mut tracker) = $config.value_source_tracker {
            tracker.set($path, $source);
        }
        $value
    };
}

/// Helper to build a source map during config loading
pub struct SourceMapBuilder {
    map: ValueSourceMap,
    config_file: Option<String>,
}

impl SourceMapBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            map: ValueSourceMap::new(),
            config_file: None,
        }
    }

    /// Set the config file path
    pub fn config_file(mut self, path: Option<String>) -> Self {
        self.config_file = path;
        self
    }

    /// Add a file value
    pub fn file_value(mut self, path: &str) -> Self {
        self.map.set(
            path,
            ValueSource::File {
                path: self.config_file.clone(),
            },
        );
        self
    }

    /// Add an environment value
    pub fn env_value(mut self, path: &str, var: &str) -> Self {
        self.map.set(
            path,
            ValueSource::Environment {
                var: var.to_string(),
            },
        );
        self
    }

    /// Add a CLI value
    pub fn cli_value(mut self, path: &str) -> Self {
        self.map.set(path, ValueSource::Cli);
        self
    }

    /// Add a default value
    pub fn default_value(mut self, path: &str) -> Self {
        self.map.set(path, ValueSource::Default);
        self
    }

    /// Build the source map
    pub fn build(self) -> ValueSourceMap {
        self.map
    }
}

impl Default for SourceMapBuilder {
    fn default() -> Self {
        Self::new()
    }
}