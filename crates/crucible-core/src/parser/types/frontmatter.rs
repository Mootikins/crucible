//! Frontmatter parsing and access

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Frontmatter metadata block
///
/// Supports both YAML (---) and TOML (+++) frontmatter formats.
/// Properties are lazily parsed to avoid allocation overhead when not accessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Raw frontmatter content (without delimiters)
    pub raw: String,

    /// Frontmatter format
    pub format: FrontmatterFormat,

    /// Lazily parsed properties
    #[serde(skip)]
    properties: OnceLock<HashMap<String, serde_json::Value>>,
}

impl Frontmatter {
    pub fn new(raw: String, format: FrontmatterFormat) -> Self {
        Self {
            raw,
            format,
            properties: OnceLock::new(),
        }
    }

    pub fn properties(&self) -> &HashMap<String, serde_json::Value> {
        self.properties.get_or_init(|| self.parse_properties())
    }

    /// Parse properties based on format
    fn parse_properties(&self) -> HashMap<String, serde_json::Value> {
        match self.format {
            FrontmatterFormat::Yaml => serde_yaml::from_str(&self.raw).unwrap_or_default(),
            FrontmatterFormat::Toml => toml::from_str(&self.raw)
                .ok()
                .and_then(|v: toml::Value| serde_json::to_value(v).ok())
                .and_then(|v| v.as_object().cloned())
                .map(|obj| obj.into_iter().collect())
                .unwrap_or_default(),
            FrontmatterFormat::None => HashMap::new(),
        }
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.properties().get(key)?.as_str().map(|s| s.to_string())
    }

    pub fn get_array(&self, key: &str) -> Option<Vec<String>> {
        self.properties()
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .into()
    }
}

/// Frontmatter format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrontmatterFormat {
    /// YAML frontmatter (---)
    Yaml,
    /// TOML frontmatter (+++)
    Toml,
    /// No frontmatter
    None,
}
