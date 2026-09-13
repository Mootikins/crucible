//! The `on_load` and `on_unload` hooks a plugin's module may carry.
//!
//! The keys live on the DAEMON VM, the one VM that runs plugin code. The
//! daemon's activation stores them from the module table it holds, then
//! calls `on_load` after the entry's `config` ran. `make_plugin_inert` calls
//! `on_unload` before it clears the plugin's registrations. Both calls take
//! the VM the keys belong to, because this manager holds no VM of its own
//! that could resolve them.
//!
//! A hook that raises is logged and recorded in the error log, and the load
//! or unload goes on: `docs/Help/Extending/Creating Plugins.md` states that
//! contract.

use super::PluginManager;
use mlua::{Function, Lua, RegistryKey};
use tracing::warn;

impl PluginManager {
    /// Remember the hooks a module table carries. A `None` drops the hook
    /// the previous generation left, so a reload that removed `on_unload`
    /// from its table does not fire the old one.
    pub fn set_lifecycle_hooks(
        &mut self,
        plugin_name: &str,
        on_load: Option<RegistryKey>,
        on_unload: Option<RegistryKey>,
    ) {
        match on_load {
            Some(key) => {
                self.on_load_hooks.insert(plugin_name.to_string(), key);
            }
            None => {
                self.on_load_hooks.remove(plugin_name);
            }
        }
        match on_unload {
            Some(key) => {
                self.on_unload_hooks.insert(plugin_name.to_string(), key);
            }
            None => {
                self.on_unload_hooks.remove(plugin_name);
            }
        }
    }

    /// Call the plugin's `on_load`, if it has one. The key stays, so a
    /// later reload that reuses the table fires it again.
    pub fn call_on_load_hook(&self, lua: &Lua, plugin_name: &str) {
        let Some(hook_key) = self.on_load_hooks.get(plugin_name) else {
            return;
        };

        match lua.registry_value::<Function>(hook_key) {
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

    /// Call the plugin's `on_unload`, if it has one, and forget it. One
    /// generation, one call: a second unload of the same generation fires
    /// nothing.
    pub fn call_on_unload_hook(&mut self, lua: &Lua, plugin_name: &str) {
        let Some(hook_key) = self.on_unload_hooks.remove(plugin_name) else {
            return;
        };

        match lua.registry_value::<Function>(&hook_key) {
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
