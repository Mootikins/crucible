use super::{LifecycleError, LifecycleResult, PluginManager};
use mlua::{Function, Value};
use tracing::warn;

impl PluginManager {
    pub(super) fn capture_on_unload_hook(
        &mut self,
        plugin_name: &str,
        plugin_spec: &mlua::Table,
    ) -> LifecycleResult<()> {
        self.on_unload_hooks.remove(plugin_name);

        if let Ok(Value::Function(on_unload)) = plugin_spec.get::<Value>("on_unload") {
            let key = self.lua.create_registry_value(on_unload).map_err(|e| {
                LifecycleError::LoadError(format!(
                    "Failed to store on_unload hook for {}: {}",
                    plugin_name, e
                ))
            })?;
            self.on_unload_hooks.insert(plugin_name.to_string(), key);
        }

        Ok(())
    }

    pub(super) fn capture_on_load_hook(
        &mut self,
        plugin_name: &str,
        plugin_spec: &mlua::Table,
    ) -> LifecycleResult<()> {
        self.on_load_hooks.remove(plugin_name);

        if let Ok(Value::Function(on_load)) = plugin_spec.get::<Value>("on_load") {
            let key = self.lua.create_registry_value(on_load).map_err(|e| {
                LifecycleError::LoadError(format!(
                    "Failed to store on_load hook for {}: {}",
                    plugin_name, e
                ))
            })?;
            self.on_load_hooks.insert(plugin_name.to_string(), key);
        }

        Ok(())
    }

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
