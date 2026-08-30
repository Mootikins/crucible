use super::{LifecycleError, LifecycleResult, PluginManager};
use mlua::{Lua, RegistryKey, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Host-owned module state for Luau's string `require` function.
///
/// PUC Lua's mutable `package.path` made module lookup an ambient global. Luau
/// intentionally has no package library, so the host owns both the active
/// plugin root and the module cache instead.
pub(super) struct ModuleResolver {
    root: Option<PathBuf>,
    modules: HashMap<(PathBuf, String), RegistryKey>,
}

impl ModuleResolver {
    pub(super) fn cache(&mut self, name: String, key: RegistryKey) {
        if let Some(root) = &self.root {
            self.modules.insert((root.clone(), name), key);
        }
    }

    pub(super) fn clear(&mut self, name: &str) {
        if let Some(root) = &self.root {
            self.modules.remove(&(root.clone(), name.to_string()));
        }
    }

    fn module_path(&self, name: &str) -> Result<PathBuf, String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| "require is unavailable outside plugin loading".to_string())?;
        if name.is_empty()
            || name.split('.').any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            })
        {
            return Err(format!("invalid plugin module name '{name}'"));
        }

        let relative = name.replace('.', "/");
        for base in [root.to_path_buf(), root.join("lua")] {
            let file = base.join(&relative).with_extension("lua");
            if file.is_file() {
                return Ok(file);
            }
            let init = base.join(&relative).join("init.lua");
            if init.is_file() {
                return Ok(init);
            }
        }
        Err(format!(
            "plugin module '{name}' was not found under {}",
            root.display()
        ))
    }
}

pub(super) fn install_module_resolver(lua: &Lua) -> mlua::Result<Arc<Mutex<ModuleResolver>>> {
    let resolver = Arc::new(Mutex::new(ModuleResolver {
        root: None,
        modules: HashMap::new(),
    }));
    let callback_resolver = Arc::clone(&resolver);
    let require = lua.create_function(move |lua, name: String| {
        let path = {
            let resolver = callback_resolver
                .lock()
                .map_err(|_| mlua::Error::runtime("plugin module resolver lock poisoned"))?;
            if let Some(root) = &resolver.root {
                if let Some(key) = resolver.modules.get(&(root.clone(), name.clone())) {
                    return lua.registry_value(key);
                }
            }
            resolver.module_path(&name).map_err(mlua::Error::runtime)?
        };

        let source = std::fs::read_to_string(&path).map_err(mlua::Error::external)?;
        let value: Value = lua
            .load(&source)
            .set_name(path.to_string_lossy().as_ref())
            .eval()?;
        let key = lua.create_registry_value(value.clone())?;
        callback_resolver
            .lock()
            .map_err(|_| mlua::Error::runtime("plugin module resolver lock poisoned"))?
            .cache(name, key);
        Ok(value)
    })?;
    lua.globals().set("require", require)?;
    Ok(resolver)
}

impl PluginManager {
    pub(super) fn configure_plugin_module_root(&self, plugin_dir: &Path) -> LifecycleResult<()> {
        let mut resolver = self.module_resolver.lock().map_err(|_| {
            LifecycleError::LoadError("plugin module resolver lock poisoned".to_string())
        })?;
        resolver.root = Some(plugin_dir.to_path_buf());
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn clear_plugin_modules(&self, plugin_name: &str) -> LifecycleResult<()> {
        self.module_resolver
            .lock()
            .map_err(|_| {
                LifecycleError::LoadError("plugin module resolver lock poisoned".to_string())
            })?
            .clear(plugin_name);
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn eval_runtime<T>(&self, source: &str) -> LifecycleResult<T>
    where
        T: mlua::FromLua,
    {
        self.lua.load(source).eval().map_err(|e| {
            LifecycleError::LoadError(format!("Failed to evaluate plugin runtime Lua: {}", e))
        })
    }
}
