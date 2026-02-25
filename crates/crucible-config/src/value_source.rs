//! Value source tracking for configuration
//!
//! This module provides functionality to track where each configuration
//! value was set (file, environment, CLI, or default).

use std::collections::HashMap;

/// Source of a configuration value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    /// Value came from the configuration file.
    File {
        /// Path to the configuration file.
        path: Option<String>,
    },
    /// Value came from an environment variable.
    Environment {
        /// Name of the environment variable.
        var: String,
    },
    /// Value came from a CLI argument
    Cli,
    /// Value is the system default
    Default,
    /// Value was overridden by a profile.
    Profile {
        /// Name of the profile.
        name: String,
    },
    /// Value was included from another file.
    Included {
        /// Path to the included file.
        file: String,
    },
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
        self.values.get(path).map(|s| s == source).unwrap_or(false)
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
    pub fn to_serializable<T>(
        &self,
        get_value: impl Fn(&str) -> Option<T>,
    ) -> HashMap<String, ValueInfo<T>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ValueSource::short() ----

    #[test]
    fn short_descriptions_all_variants() {
        assert_eq!(ValueSource::File { path: None }.short(), "file");
        assert_eq!(
            ValueSource::Environment {
                var: "X".to_string()
            }
            .short(),
            "env"
        );
        assert_eq!(ValueSource::Cli.short(), "cli");
        assert_eq!(ValueSource::Default.short(), "default");
        assert_eq!(
            ValueSource::Profile {
                name: "p".to_string()
            }
            .short(),
            "profile"
        );
        assert_eq!(
            ValueSource::Included {
                file: "f".to_string()
            }
            .short(),
            "included"
        );
    }

    // ---- ValueSource::detail() ----

    #[test]
    fn detail_file_with_path() {
        let src = ValueSource::File {
            path: Some("/etc/crucible.toml".to_string()),
        };
        let detail = src.detail();
        assert!(detail.contains("file"), "detail should mention 'file'");
        assert!(
            detail.contains("/etc/crucible.toml"),
            "detail should contain the path"
        );
    }

    #[test]
    fn detail_file_without_path() {
        let src = ValueSource::File { path: None };
        assert_eq!(src.detail(), "file");
    }

    #[test]
    fn detail_environment() {
        let src = ValueSource::Environment {
            var: "FOO".to_string(),
        };
        let detail = src.detail();
        assert!(
            detail.contains("environment"),
            "detail should mention 'environment'"
        );
        assert!(detail.contains("FOO"), "detail should contain var name");
    }

    #[test]
    fn detail_cli() {
        let detail = ValueSource::Cli.detail();
        assert!(detail.contains("cli"), "detail should mention 'cli'");
    }

    #[test]
    fn detail_default() {
        let detail = ValueSource::Default.detail();
        assert!(
            detail.contains("default"),
            "detail should mention 'default'"
        );
    }

    #[test]
    fn detail_profile() {
        let src = ValueSource::Profile {
            name: "dev".to_string(),
        };
        let detail = src.detail();
        assert!(
            detail.contains("profile"),
            "detail should mention 'profile'"
        );
        assert!(
            detail.contains("dev"),
            "detail should contain profile name"
        );
    }

    #[test]
    fn detail_included() {
        let src = ValueSource::Included {
            file: "mcps.toml".to_string(),
        };
        let detail = src.detail();
        assert!(
            detail.contains("included"),
            "detail should mention 'included'"
        );
        assert!(
            detail.contains("mcps.toml"),
            "detail should contain included file"
        );
    }

    // ---- ValueSourceMap ----

    #[test]
    fn map_set_and_get() {
        let mut map = ValueSourceMap::new();
        let src = ValueSource::Environment {
            var: "EMBEDDING_PROVIDER".to_string(),
        };
        map.set("embedding.provider", src.clone());

        let got = map.get("embedding.provider");
        assert_eq!(got, Some(&src));
    }

    #[test]
    fn map_has_source_match() {
        let mut map = ValueSourceMap::new();
        let src = ValueSource::Cli;
        map.set("kiln_path", src.clone());

        assert!(map.has_source("kiln_path", &ValueSource::Cli));
    }

    #[test]
    fn map_has_source_no_match() {
        let mut map = ValueSourceMap::new();
        map.set("kiln_path", ValueSource::Cli);

        assert!(!map.has_source("kiln_path", &ValueSource::Default));
    }

    #[test]
    fn map_values_from_source() {
        let mut map = ValueSourceMap::new();
        let file_src = ValueSource::File { path: None };
        map.set("a.b", file_src.clone());
        map.set("c.d", ValueSource::Cli);
        map.set("e.f", file_src.clone());

        let mut paths = map.values_from_source(&file_src);
        paths.sort();
        assert_eq!(paths, vec!["a.b", "e.f"]);
    }

    #[test]
    fn map_to_serializable() {
        let mut map = ValueSourceMap::new();
        map.set(
            "kiln_path",
            ValueSource::File {
                path: Some("/home/user/.config/crucible/config.toml".to_string()),
            },
        );
        map.set("chat.model", ValueSource::Default);

        let serializable = map.to_serializable(|path| match path {
            "kiln_path" => Some("~/notes".to_string()),
            "chat.model" => Some("gpt-4o".to_string()),
            _ => None,
        });

        let kiln_info = serializable.get("kiln_path").expect("kiln_path missing");
        assert_eq!(kiln_info.value, "~/notes");
        assert!(kiln_info.source.contains("file"));
        assert_eq!(kiln_info.source_short, "file");

        let chat_info = serializable.get("chat.model").expect("chat.model missing");
        assert_eq!(chat_info.value, "gpt-4o");
        assert!(chat_info.source.contains("default"));
        assert_eq!(chat_info.source_short, "default");
    }
}
