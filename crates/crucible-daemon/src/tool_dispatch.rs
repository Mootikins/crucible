use async_trait::async_trait;
use crucible_core::traits::tools::{ExecutionContext, ToolExecutor};
use crate::tools::workspace::WorkspaceTools;
use std::sync::Arc;

#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    async fn dispatch_tool(&self, name: &str, args: serde_json::Value)
        -> Result<serde_json::Value, String>;
    fn has_tool(&self, name: &str) -> bool;
}

pub struct DaemonToolDispatcher {
    workspace_tools: Arc<WorkspaceTools>,
}

impl DaemonToolDispatcher {
    pub fn new(workspace_tools: Arc<WorkspaceTools>) -> Self {
        Self { workspace_tools }
    }
}

#[async_trait]
impl ToolDispatcher for DaemonToolDispatcher {
    async fn dispatch_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let ctx = ExecutionContext::default();
        self.workspace_tools
            .execute_tool(name, args, &ctx)
            .await
            .map_err(|e| e.to_string())
    }

    fn has_tool(&self, name: &str) -> bool {
        WorkspaceTools::tool_definitions()
            .iter()
            .any(|tool| tool.name.as_ref() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_tool_dispatcher_construction() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            &std::path::PathBuf::from("/tmp"),
        ));

        let dispatcher = DaemonToolDispatcher::new(workspace_tools.clone());

        assert!(std::mem::size_of_val(&dispatcher) > 0);
    }

    #[test]
    fn test_daemon_tool_dispatcher_holds_workspace_tools_arc() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            &std::path::PathBuf::from("/tmp"),
        ));

        let strong_count = Arc::strong_count(&workspace_tools);

        let _dispatcher = DaemonToolDispatcher::new(workspace_tools.clone());

        assert_eq!(Arc::strong_count(&workspace_tools), strong_count + 1);
    }

    #[test]
    fn test_has_tool_checks_workspace_definitions() {
        let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
            &std::path::PathBuf::from("/tmp"),
        ));
        let dispatcher = DaemonToolDispatcher::new(workspace_tools);

        assert!(dispatcher.has_tool("read_file"));
        assert!(!dispatcher.has_tool("not_a_tool"));
    }
}
