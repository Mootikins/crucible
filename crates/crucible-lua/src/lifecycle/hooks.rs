//! The `on_load` and `on_unload` hooks a plugin's module may carry.
//!
//! Nothing fills `on_load_hooks` or `on_unload_hooks` today: the manager
//! VM used to capture them when it evaluated `init.luau`, and it evaluates
//! nothing now. The two calls stay so `load` and `unload` keep their shape
//! until the daemon VM supplies the keys.

use super::PluginManager;
use mlua::Function;
use tracing::warn;

impl PluginManager {
    pub(super) fn call_on_load_hook(&self, plugin_name: &str) {
        let Some(hook_key) = self.on_load_hooks.get(plugin_name) else {
            return;
        };

        match self.lua.registry_value::<Function>(hook_key) {
            Ok(on_load) => {
                if let Err(error) = on_load.call::<()>(()) {
                    warn!("on_load hook failed for {}: {}", plugin_name, error);
                    self.capture_plugin_error(
                        plugin_name,
                        &error,
                        format!("handler:on_load:{}", plugin_name),
                    );
                }
            }
            Err(error) => {
                warn!(
                    "Failed to retrieve on_load hook for {}: {}",
                    plugin_name, error
                );
            }
        }
    }

    pub(super) fn call_on_unload_hook(&mut self, plugin_name: &str) {
        let Some(hook_key) = self.on_unload_hooks.remove(plugin_name) else {
            return;
        };

        match self.lua.registry_value::<Function>(&hook_key) {
            Ok(on_unload) => {
                if let Err(error) = on_unload.call::<()>(()) {
                    warn!("on_unload hook failed for {}: {}", plugin_name, error);
                    self.capture_plugin_error(
                        plugin_name,
                        &error,
                        format!("handler:on_unload:{}", plugin_name),
                    );
                }
            }
            Err(error) => {
                warn!(
                    "Failed to retrieve on_unload hook for {}: {}",
                    plugin_name, error
                );
            }
        }
    }
}
