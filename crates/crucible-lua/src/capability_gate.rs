//! Capability enforcement for plugin module registration
//!
//! This module provides capability checking logic that gates module registration
//! based on declared plugin capabilities. In v1, undeclared usage triggers warnings
//! but doesn't deny access. Strict mode can be enabled for enforcement.

use crate::manifest::{Capability, PluginManifest};
use tracing::warn;

/// Mapping from module name to required capability
pub struct ModuleCapabilityMapping {
    pub module_name: &'static str,
    pub required_capability: Option<Capability>,
}

/// All module-to-capability mappings
///
/// Modules with `None` capability are always available (core functionality).
/// Modules with `Some(cap)` require the plugin to declare that capability.
pub fn module_capability_map() -> Vec<ModuleCapabilityMapping> {
    vec![
        ModuleCapabilityMapping {
            module_name: "http",
            required_capability: Some(Capability::Network),
        },
        ModuleCapabilityMapping {
            module_name: "ws",
            required_capability: Some(Capability::WebSocket),
        },
        ModuleCapabilityMapping {
            module_name: "shell",
            required_capability: Some(Capability::Shell),
        },
        ModuleCapabilityMapping {
            module_name: "fs",
            required_capability: Some(Capability::Filesystem),
        },
        ModuleCapabilityMapping {
            module_name: "kiln",
            required_capability: Some(Capability::Kiln),
        },
        ModuleCapabilityMapping {
            module_name: "graph",
            required_capability: Some(Capability::Kiln),
        },
        // No capability needed for core modules:
        ModuleCapabilityMapping {
            module_name: "sessions",
            required_capability: None,
        },
        ModuleCapabilityMapping {
            module_name: "json_query",
            required_capability: None,
        },
        ModuleCapabilityMapping {
            module_name: "paths",
            required_capability: None,
        },
    ]
}

/// Check if a plugin should have access to a module.
///
/// Returns `(allowed: bool, warning: Option<String>)`:
/// - `allowed`: Whether the module should be registered
/// - `warning`: Optional warning message if capability is missing
///
/// # Arguments
///
/// * `plugin_name` - Name of the plugin requesting access
/// * `module_name` - Name of the module being registered (e.g., "http", "fs")
/// * `manifest` - Plugin manifest containing declared capabilities
/// * `strict` - If true, deny access to undeclared capabilities; if false, warn only
///
/// # Behavior
///
/// - Modules with no capability requirement are always allowed
/// - Modules with capability requirement check `manifest.has_capability()`
/// - In non-strict mode: missing capability → warning logged, access granted
/// - In strict mode: missing capability → warning returned, access denied
/// - Unknown modules default to allowed (forward compatibility)
pub fn check_module_access(
    plugin_name: &str,
    module_name: &str,
    manifest: &PluginManifest,
    strict: bool,
) -> (bool, Option<String>) {
    let mapping = module_capability_map();
    let entry = mapping.iter().find(|m| m.module_name == module_name);

    match entry {
        Some(m) => match &m.required_capability {
            None => (true, None), // No capability needed
            Some(cap) => {
                if manifest.has_capability(*cap) {
                    (true, None)
                } else {
                    let warning = format!(
                        "Plugin '{}' uses cru.{} without declaring '{}' capability",
                        plugin_name,
                        module_name,
                        cap.description()
                    );
                    if strict {
                        (false, Some(warning))
                    } else {
                        warn!("{}", warning);
                        (true, Some(warning))
                    }
                }
            }
        },
        None => (true, None), // Unknown module, allow by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(name: &str, capabilities: Vec<Capability>) -> PluginManifest {
        PluginManifest {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: None,
            main: "init.lua".to_string(),
            init: None,
            capabilities,
            dependencies: Vec::new(),
            exports: Default::default(),
            config: None,
            enabled: None,
        }
    }

    #[test]
    fn plugin_with_network_cap_gets_http() {
        let manifest = make_manifest("test-plugin", vec![Capability::Network]);
        let (allowed, warning) = check_module_access("test-plugin", "http", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn plugin_without_network_cap_gets_warning() {
        let manifest = make_manifest("test-plugin", vec![]);
        let (allowed, warning) = check_module_access("test-plugin", "http", &manifest, false);
        assert!(allowed);
        assert!(warning.is_some());
        let msg = warning.unwrap();
        assert!(msg.contains("test-plugin"));
        assert!(msg.contains("http"));
        assert!(msg.contains("Make HTTP requests"));
    }

    #[test]
    fn strict_mode_denies_undeclared() {
        let manifest = make_manifest("test-plugin", vec![]);
        let (allowed, warning) = check_module_access("test-plugin", "http", &manifest, true);
        assert!(!allowed);
        assert!(warning.is_some());
        let msg = warning.unwrap();
        assert!(msg.contains("test-plugin"));
        assert!(msg.contains("http"));
        assert!(msg.contains("Make HTTP requests"));
    }

    #[test]
    fn no_cap_needed_for_sessions() {
        let manifest = make_manifest("test-plugin", vec![]);
        let (allowed, warning) = check_module_access("test-plugin", "sessions", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn no_cap_needed_for_json_query() {
        let manifest = make_manifest("test-plugin", vec![]);
        let (allowed, warning) = check_module_access("test-plugin", "json_query", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn websocket_cap_required_for_ws() {
        let manifest = make_manifest("test-plugin", vec![Capability::WebSocket]);
        let (allowed, warning) = check_module_access("test-plugin", "ws", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());

        // Without WebSocket capability
        let manifest_no_ws = make_manifest("test-plugin", vec![]);
        let (allowed, warning) = check_module_access("test-plugin", "ws", &manifest_no_ws, false);
        assert!(allowed); // Non-strict allows
        assert!(warning.is_some());
    }

    #[test]
    fn filesystem_cap_required_for_fs() {
        let manifest = make_manifest("test-plugin", vec![Capability::Filesystem]);
        let (allowed, warning) = check_module_access("test-plugin", "fs", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn shell_cap_required_for_shell() {
        let manifest = make_manifest("test-plugin", vec![Capability::Shell]);
        let (allowed, warning) = check_module_access("test-plugin", "shell", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn kiln_cap_required_for_kiln() {
        let manifest = make_manifest("test-plugin", vec![Capability::Kiln]);
        let (allowed, warning) = check_module_access("test-plugin", "kiln", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn kiln_cap_required_for_graph() {
        let manifest = make_manifest("test-plugin", vec![Capability::Kiln]);
        let (allowed, warning) = check_module_access("test-plugin", "graph", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn unknown_module_allowed_by_default() {
        let manifest = make_manifest("test-plugin", vec![]);
        let (allowed, warning) =
            check_module_access("test-plugin", "unknown_module", &manifest, false);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn strict_mode_with_declared_capability() {
        let manifest = make_manifest("test-plugin", vec![Capability::Network]);
        let (allowed, warning) = check_module_access("test-plugin", "http", &manifest, true);
        assert!(allowed);
        assert!(warning.is_none());
    }

    #[test]
    fn multiple_capabilities_work() {
        let manifest = make_manifest(
            "test-plugin",
            vec![
                Capability::Network,
                Capability::Filesystem,
                Capability::Shell,
            ],
        );

        let (allowed, _) = check_module_access("test-plugin", "http", &manifest, true);
        assert!(allowed);

        let (allowed, _) = check_module_access("test-plugin", "fs", &manifest, true);
        assert!(allowed);

        let (allowed, _) = check_module_access("test-plugin", "shell", &manifest, true);
        assert!(allowed);

        // But not WebSocket
        let (allowed, warning) = check_module_access("test-plugin", "ws", &manifest, true);
        assert!(!allowed);
        assert!(warning.is_some());
    }
}
