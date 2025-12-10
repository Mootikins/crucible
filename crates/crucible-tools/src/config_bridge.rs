//! Bridge between crucible-config and crucible-rune discovery types
//!
//! This module provides conversion utilities to connect configuration types
//! from `crucible-config` (TOML-based) with runtime types from `crucible-rune`.
//!
//! The types have identical fields but are separate because:
//! - `crucible-config::TypeDiscoveryConfig` - TOML config (has `additional_paths`, `use_defaults`)
//! - `crucible-rune::DiscoveryConfig` - Runtime config (has same fields)
//!
//! `crucible-rune` doesn't depend on `crucible-config` to avoid circular dependencies,
//! so this bridge module in `crucible-tools` (which depends on both) provides the connection.

use crucible_config::{DiscoveryPathsConfig, TypeDiscoveryConfig};
use crucible_rune::{DiscoveryConfig, DiscoveryPaths, EventHandlerConfig, RuneDiscoveryConfig};
use std::path::Path;

/// Convert `TypeDiscoveryConfig` (from crucible-config) to `DiscoveryConfig` (from crucible-rune)
///
/// These types have identical fields, but are separate to avoid cross-crate dependencies.
///
/// # Example
///
/// ```rust
/// use crucible_config::TypeDiscoveryConfig;
/// use crucible_tools::to_rune_discovery_config;
/// use std::path::PathBuf;
///
/// let config = TypeDiscoveryConfig {
///     additional_paths: vec![PathBuf::from("/custom/path")],
///     use_defaults: true,
/// };
///
/// let rune_config = to_rune_discovery_config(&config);
/// assert_eq!(rune_config.additional_paths.len(), 1);
/// assert!(rune_config.use_defaults);
/// ```
pub fn to_rune_discovery_config(config: &TypeDiscoveryConfig) -> DiscoveryConfig {
    DiscoveryConfig {
        additional_paths: config.additional_paths.clone(),
        use_defaults: config.use_defaults,
    }
}

/// Create `DiscoveryPaths` from config, falling back to defaults
///
/// This function looks up the configuration for a specific type (e.g., "tools", "hooks", "events")
/// and creates the appropriate `DiscoveryPaths` instance.
///
/// # Arguments
///
/// * `type_name` - The resource type (e.g., "tools", "hooks", "events")
/// * `kiln_path` - Optional path to the kiln directory
/// * `config` - Optional discovery paths configuration from TOML
///
/// # Example
///
/// ```rust
/// use crucible_config::{DiscoveryPathsConfig, TypeDiscoveryConfig};
/// use crucible_tools::create_discovery_paths;
/// use std::path::PathBuf;
///
/// let config = DiscoveryPathsConfig {
///     tools: Some(TypeDiscoveryConfig {
///         additional_paths: vec![PathBuf::from("/opt/tools")],
///         use_defaults: true,
///     }),
///     hooks: None,
///     events: None,
///     type_configs: Default::default(),
/// };
///
/// let paths = create_discovery_paths("tools", None, Some(&config));
/// assert!(paths.additional_paths().contains(&PathBuf::from("/opt/tools")));
/// ```
pub fn create_discovery_paths(
    type_name: &str,
    kiln_path: Option<&Path>,
    config: Option<&DiscoveryPathsConfig>,
) -> DiscoveryPaths {
    match config.and_then(|c| c.get_type_config(type_name)) {
        Some(type_config) => {
            let rune_config = to_rune_discovery_config(type_config);
            DiscoveryPaths::from_config(type_name, kiln_path, &rune_config)
        }
        None => DiscoveryPaths::new(type_name, kiln_path),
    }
}

/// Create `RuneDiscoveryConfig` from `DiscoveryPathsConfig`
///
/// This creates the configuration needed for Rune tool discovery,
/// respecting any custom paths and defaults from the config file.
///
/// # Arguments
///
/// * `kiln_path` - Optional path to the kiln directory
/// * `config` - Optional discovery paths configuration from TOML
///
/// # Example
///
/// ```rust
/// use crucible_config::{DiscoveryPathsConfig, TypeDiscoveryConfig};
/// use crucible_tools::create_rune_discovery_config;
/// use std::path::PathBuf;
///
/// let config = DiscoveryPathsConfig {
///     tools: Some(TypeDiscoveryConfig {
///         additional_paths: vec![PathBuf::from("/opt/tools")],
///         use_defaults: true,
///     }),
///     ..Default::default()
/// };
///
/// let rune_config = create_rune_discovery_config(None, Some(&config));
/// assert!(rune_config.tool_directories.contains(&PathBuf::from("/opt/tools")));
/// ```
pub fn create_rune_discovery_config(
    kiln_path: Option<&Path>,
    config: Option<&DiscoveryPathsConfig>,
) -> RuneDiscoveryConfig {
    let paths = create_discovery_paths("tools", kiln_path, config);
    RuneDiscoveryConfig::from_discovery_paths(&paths)
}

/// Create `EventHandlerConfig` from `DiscoveryPathsConfig`
///
/// This creates the configuration needed for event handler discovery,
/// respecting any custom paths and defaults from the config file.
///
/// # Arguments
///
/// * `kiln_path` - Optional path to the kiln directory
/// * `config` - Optional discovery paths configuration from TOML
///
/// # Example
///
/// ```rust
/// use crucible_config::{DiscoveryPathsConfig, TypeDiscoveryConfig};
/// use crucible_tools::create_event_handler_config;
/// use std::path::PathBuf;
///
/// let config = DiscoveryPathsConfig {
///     events: Some(TypeDiscoveryConfig {
///         additional_paths: vec![PathBuf::from("/opt/events")],
///         use_defaults: true,
///     }),
///     ..Default::default()
/// };
///
/// let event_config = create_event_handler_config(None, Some(&config));
/// // event_config now uses the custom paths
/// ```
pub fn create_event_handler_config(
    kiln_path: Option<&Path>,
    config: Option<&DiscoveryPathsConfig>,
) -> EventHandlerConfig {
    let paths = create_discovery_paths("events", kiln_path, config);
    EventHandlerConfig::from_discovery_paths(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_convert_type_discovery_config_to_rune_discovery_config() {
        let config = TypeDiscoveryConfig {
            additional_paths: vec![PathBuf::from("/custom/path")],
            use_defaults: false,
        };

        let rune_config = to_rune_discovery_config(&config);

        assert_eq!(rune_config.additional_paths.len(), 1);
        assert_eq!(
            rune_config.additional_paths[0],
            PathBuf::from("/custom/path")
        );
        assert!(!rune_config.use_defaults);
    }

    #[test]
    fn test_convert_type_discovery_config_with_defaults() {
        let config = TypeDiscoveryConfig {
            additional_paths: vec![],
            use_defaults: true,
        };

        let rune_config = to_rune_discovery_config(&config);

        assert!(rune_config.additional_paths.is_empty());
        assert!(rune_config.use_defaults);
    }

    #[test]
    fn test_convert_type_discovery_config_multiple_paths() {
        let config = TypeDiscoveryConfig {
            additional_paths: vec![
                PathBuf::from("/path1"),
                PathBuf::from("/path2"),
                PathBuf::from("/path3"),
            ],
            use_defaults: true,
        };

        let rune_config = to_rune_discovery_config(&config);

        assert_eq!(rune_config.additional_paths.len(), 3);
        assert!(rune_config.use_defaults);
    }

    #[test]
    fn test_create_discovery_paths_from_config() {
        let config = DiscoveryPathsConfig {
            tools: Some(TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/opt/tools")],
                use_defaults: true,
            }),
            hooks: None,
            events: None,
            type_configs: Default::default(),
        };

        let paths = create_discovery_paths("tools", None, Some(&config));

        assert!(paths.additional_paths().contains(&PathBuf::from("/opt/tools")));
        assert!(paths.uses_defaults());
    }

    #[test]
    fn test_create_discovery_paths_for_hooks() {
        let config = DiscoveryPathsConfig {
            tools: None,
            hooks: Some(TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/custom/hooks")],
                use_defaults: false,
            }),
            events: None,
            type_configs: Default::default(),
        };

        let paths = create_discovery_paths("hooks", None, Some(&config));

        assert!(paths
            .additional_paths()
            .contains(&PathBuf::from("/custom/hooks")));
        assert!(!paths.uses_defaults());
    }

    #[test]
    fn test_create_discovery_paths_for_events() {
        let config = DiscoveryPathsConfig {
            tools: None,
            hooks: None,
            events: Some(TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/custom/events")],
                use_defaults: true,
            }),
            type_configs: Default::default(),
        };

        let paths = create_discovery_paths("events", None, Some(&config));

        assert!(paths
            .additional_paths()
            .contains(&PathBuf::from("/custom/events")));
        assert!(paths.uses_defaults());
    }

    #[test]
    fn test_create_discovery_paths_without_config() {
        let paths = create_discovery_paths("tools", None, None);

        // Should use defaults when no config provided
        assert!(paths.uses_defaults());
        assert!(paths.additional_paths().is_empty());
    }

    #[test]
    fn test_create_discovery_paths_with_empty_config() {
        let config = DiscoveryPathsConfig::default();

        let paths = create_discovery_paths("tools", None, Some(&config));

        // Should use defaults when config has no entry for this type
        assert!(paths.uses_defaults());
        assert!(paths.additional_paths().is_empty());
    }

    #[test]
    fn test_create_discovery_paths_with_kiln_path() {
        let kiln_path = PathBuf::from("/my/kiln");
        let config = DiscoveryPathsConfig {
            tools: Some(TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/extra/tools")],
                use_defaults: true,
            }),
            hooks: None,
            events: None,
            type_configs: Default::default(),
        };

        let paths = create_discovery_paths("tools", Some(&kiln_path), Some(&config));

        // Should have additional paths
        assert!(paths
            .additional_paths()
            .contains(&PathBuf::from("/extra/tools")));
        // Should use defaults (which includes kiln-specific path)
        assert!(paths.uses_defaults());
        // Default paths should include kiln-specific path
        let kiln_tools = kiln_path.join(".crucible").join("tools");
        assert!(paths.default_paths().contains(&kiln_tools));
    }

    #[test]
    fn test_create_rune_discovery_config_from_config() {
        let config = DiscoveryPathsConfig {
            tools: Some(TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/opt/rune/tools")],
                use_defaults: true,
            }),
            hooks: None,
            events: None,
            type_configs: Default::default(),
        };

        let rune_config = create_rune_discovery_config(None, Some(&config));

        // Should include the additional paths
        assert!(rune_config
            .tool_directories
            .contains(&PathBuf::from("/opt/rune/tools")));
    }

    #[test]
    fn test_create_rune_discovery_config_without_config() {
        let rune_config = create_rune_discovery_config(None, None);

        // Should have default extensions
        assert!(rune_config.extensions.contains(&"rn".to_string()));
        assert!(rune_config.recursive);
    }

    #[test]
    fn test_create_event_handler_config_from_config() {
        let config = DiscoveryPathsConfig {
            tools: None,
            hooks: None,
            events: Some(TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/opt/events")],
                use_defaults: true,
            }),
            type_configs: Default::default(),
        };

        let event_config = create_event_handler_config(None, Some(&config));

        // EventHandlerConfig wraps DiscoveryPaths, verify base directories
        let base_dirs = event_config.base_directories();
        assert!(base_dirs.contains(&PathBuf::from("/opt/events")));
    }

    #[test]
    fn test_create_event_handler_config_without_config() {
        let event_config = create_event_handler_config(None, None);

        // Should have default directories (global ~/.crucible/events/)
        let base_dirs = event_config.base_directories();
        // At minimum should have some default paths if home dir exists
        // (exact paths depend on system)
        assert!(base_dirs.is_empty() || base_dirs.iter().any(|p| p.ends_with("events")));
    }

    #[test]
    fn test_create_discovery_paths_type_configs_fallback() {
        use std::collections::HashMap;

        // Test that type_configs (nested format) works as fallback
        let mut type_configs = HashMap::new();
        type_configs.insert(
            "custom_type".to_string(),
            TypeDiscoveryConfig {
                additional_paths: vec![PathBuf::from("/custom/type/path")],
                use_defaults: false,
            },
        );

        let config = DiscoveryPathsConfig {
            tools: None,
            hooks: None,
            events: None,
            type_configs,
        };

        let paths = create_discovery_paths("custom_type", None, Some(&config));

        assert!(paths
            .additional_paths()
            .contains(&PathBuf::from("/custom/type/path")));
        assert!(!paths.uses_defaults());
    }
}
