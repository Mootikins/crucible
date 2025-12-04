//! Frontmatter parsing and access

use chrono::NaiveDate;
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
    /// Create new frontmatter from raw string
    pub fn new(raw: String, format: FrontmatterFormat) -> Self {
        Self {
            raw,
            format,
            properties: OnceLock::new(),
        }
    }

    /// Get parsed properties (lazy initialization)
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

    /// Get a string property
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.properties().get(key)?.as_str().map(|s| s.to_string())
    }

    /// Get an array property
    pub fn get_array(&self, key: &str) -> Option<Vec<String>> {
        self.properties()
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .into()
    }

    /// Get a boolean property
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.properties().get(key)?.as_bool()
    }

    /// Get a number property
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.properties().get(key)?.as_f64()
    }

    /// Get a date property
    ///
    /// Supports multiple date formats:
    /// - ISO 8601: "2024-11-08"
    /// - RFC 3339: "2024-11-08T10:30:00Z"
    /// - Integer (YYYYMMDD): 20241108
    pub fn get_date(&self, key: &str) -> Option<NaiveDate> {
        let value = self.properties().get(key)?;

        // Try as string first (most common format)
        if let Some(date_str) = value.as_str() {
            // Try ISO 8601 format (YYYY-MM-DD)
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                return Some(date);
            }
            // Try RFC 3339 format (with time)
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
                return Some(dt.date_naive());
            }
        }

        // Try as integer (YYYYMMDD format)
        if let Some(num) = value.as_i64() {
            let year = (num / 10000) as i32;
            let month = ((num % 10000) / 100) as u32;
            let day = (num % 100) as u32;
            return NaiveDate::from_ymd_opt(year, month, day);
        }

        None
    }

    /// Get an object (nested hash map) property
    ///
    /// Returns the object as a serde_json::Map for further processing.
    /// Note: Flat frontmatter structure is preferred (following Obsidian conventions),
    /// but objects are supported for compatibility with existing content.
    pub fn get_object(&self, key: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        self.properties().get(key)?.as_object().cloned()
    }

    /// Check if a property exists
    pub fn has(&self, key: &str) -> bool {
        self.properties().contains_key(key)
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
