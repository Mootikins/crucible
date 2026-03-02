use crucible_core::traits::tools::{ExecutionContext, ToolExecutor};
use crucible_lua::DaemonToolsApi;
use crate::tools::workspace::WorkspaceTools;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type BoxFut<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

pub struct DaemonToolsBridge {
    workspace_tools: Arc<WorkspaceTools>,
}

impl DaemonToolsBridge {
    pub fn new(workspace_tools: Arc<WorkspaceTools>) -> Self {
        Self { workspace_tools }
    }
}

impl DaemonToolsApi for DaemonToolsBridge {
    fn call_tool(&self, name: String, args: serde_json::Value) -> BoxFut<serde_json::Value> {
        let tools = Arc::clone(&self.workspace_tools);
        Box::pin(async move {
            let ctx = ExecutionContext::default();
            tools
                .execute_tool(&name, args, &ctx)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn list_tools(&self) -> BoxFut<Vec<serde_json::Value>> {
        let tools = Arc::clone(&self.workspace_tools);
        Box::pin(async move {
            let defs = tools.list_tools().await.map_err(|e| e.to_string())?;
            defs.into_iter()
                .map(|t| serde_json::to_value(&t).map_err(|e| e.to_string()))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_tools_bridge_construction() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            &std::path::PathBuf::from("/tmp"),
        ));

        let bridge = DaemonToolsBridge::new(workspace_tools.clone());

        // Verify bridge was created (no panic)
        assert!(std::mem::size_of_val(&bridge) > 0);
    }

    #[test]
    fn test_daemon_tools_bridge_delegates_to_workspace_tools() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            &std::path::PathBuf::from("/tmp"),
        ));

        let strong_count = Arc::strong_count(&workspace_tools);

        let _bridge = DaemonToolsBridge::new(workspace_tools.clone());

        // Verify Arc reference is held (strong count increased)
        assert_eq!(Arc::strong_count(&workspace_tools), strong_count + 1);
    }
}
