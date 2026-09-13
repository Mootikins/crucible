use super::{LifecycleError, LifecycleResult, PluginManager};

impl PluginManager {
    /// What a test reads out of the manager VM. The VM runs no plugin code,
    /// so this answers only what the test itself put there.
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
