pub mod runtime;

use anyhow::Result;

pub struct PluginManager {
    runtime: runtime::RuneRuntime,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            runtime: runtime::RuneRuntime::new(),
        }
    }

    pub async fn load_plugin(&mut self, path: &str) -> Result<()> {
        self.runtime.load_script(path).await
    }

    pub async fn execute_command(
        &self,
        command: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.runtime.execute_command(command, args).await
    }
}
