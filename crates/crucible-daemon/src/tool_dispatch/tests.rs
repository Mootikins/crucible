//! Tests for [`super::DaemonToolDispatcher`].
//!
//! Split out of `tool_dispatch.rs` to keep that file inside the 1000-line module
//! budget enforced by `no_new_oversized_modules`.

mod dispatch {
    use super::super::*;
    use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
    use crate::tools::mcp_server::CrucibleMcpServer;
    use crate::tools::workspace::WorkspaceTools;
    use async_trait::async_trait;
    use serde_json::json;
    use tempfile::TempDir;

    fn workspace_tools() -> Arc<WorkspaceTools> {
        Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp")))
    }

    fn test_dispatcher() -> DaemonToolDispatcher {
        DaemonToolDispatcher::new(vec![workspace_tools() as Arc<dyn ToolExecutor>])
    }

    fn test_dispatcher_with_mcp() -> (TempDir, DaemonToolDispatcher) {
        let temp = TempDir::new().expect("tempdir");
        std::fs::write(temp.path().join("test.md"), "hello world\nsearch test\n")
            .expect("seed note");

        let mcp_server = Arc::new(CrucibleMcpServer::new(
            temp.path().display().to_string(),
            Arc::new(EmptyKnowledgeRepository),
            Arc::new(EmptyEmbeddingProvider),
        ));

        let providers: Vec<Arc<dyn ToolExecutor>> = vec![
            workspace_tools(),
            Arc::new(McpToolExecutor::new(mcp_server)),
        ];

        (temp, DaemonToolDispatcher::new(providers))
    }

    #[test]
    fn test_daemon_tool_dispatcher_construction() {
        let workspace_tools = workspace_tools();
        let dispatcher = DaemonToolDispatcher::new(vec![workspace_tools.clone()]);

        assert!(std::mem::size_of_val(&dispatcher) > 0);
    }

    #[test]
    fn test_daemon_tool_dispatcher_holds_workspace_tools_arc() {
        let workspace_tools = workspace_tools();
        let strong_count = Arc::strong_count(&workspace_tools);

        let _dispatcher = DaemonToolDispatcher::new(vec![workspace_tools.clone()]);

        assert_eq!(Arc::strong_count(&workspace_tools), strong_count + 1);
    }

    #[test]
    fn test_has_tool_checks_workspace_definitions() {
        let dispatcher = test_dispatcher();

        assert!(dispatcher.has_tool("read_file"));
        assert!(!dispatcher.has_tool("not_a_tool"));
    }

    #[tokio::test]
    async fn red_dispatch_get_kiln_info_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("get_kiln_info", json!({}), Default::default())
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "get_kiln_info should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_list_notes_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("list_notes", json!({}), Default::default())
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "list_notes should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_read_note_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "read_note",
                json!({ "path": "test.md" }),
                Default::default(),
            )
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "read_note should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_text_search_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "text_search",
                json!({ "query": "test" }),
                Default::default(),
            )
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "text_search should not route to Unknown tool error: {result:?}"
        );
    }

    #[test]
    fn red_has_tool_reports_get_kiln_info() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();

        assert!(dispatcher.has_tool("get_kiln_info"));
    }

    #[tokio::test]
    async fn workspace_tools_still_dispatch_with_mcp_provider_present() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("glob", json!({ "pattern": "**/*.md" }), Default::default())
            .await;

        assert!(
            result.is_ok(),
            "workspace tool should still dispatch: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_discover_tools_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("discover_tools", json!({}), Default::default())
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "discover_tools should be handled by the bridge, not Unknown: {result:?}"
        );
        let value = result.expect("discover_tools should succeed");
        let rendered = value.to_string();
        assert!(
            rendered.contains("read_file") || rendered.contains("get_kiln_info"),
            "discovery output should list provider tools: {rendered}"
        );
    }

    #[tokio::test]
    async fn dispatch_get_tool_schema_returns_schema_for_known_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "get_tool_schema",
                json!({ "name": "read_file" }),
                Default::default(),
            )
            .await
            .expect("get_tool_schema should succeed for a known tool");

        assert!(
            result.to_string().contains("read_file"),
            "schema output should name the tool: {result}"
        );
    }

    #[tokio::test]
    async fn dispatch_get_tool_schema_errors_for_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "get_tool_schema",
                json!({ "name": "does_not_exist" }),
                Default::default(),
            )
            .await;

        assert!(result.is_err(), "unknown tool schema lookup should error");
    }

    /// Surfaces come from the provider that would actually run the tool.
    ///
    /// Cheapest place to catch a misclassification: get this wrong and either
    /// the sandbox has a hole (`bash` reported `Daemon`) or a sandboxed session
    /// loses its knowledge tools (`get_kiln_info` reported `Host`), and both
    /// only show up under a live isolation claim otherwise.
    #[tokio::test]
    async fn tool_surfaces_follow_the_dispatching_provider() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();

        assert_eq!(dispatcher.tool_surface("bash").await, ToolSurface::Host);
        assert_eq!(
            dispatcher.tool_surface("read_file").await,
            ToolSurface::Host
        );
        assert_eq!(
            dispatcher.tool_surface("get_kiln_info").await,
            ToolSurface::Daemon
        );
        assert_eq!(
            dispatcher.tool_surface("semantic_search").await,
            ToolSurface::Daemon
        );
    }

    /// The other one-line edit that opens every sandbox.
    ///
    /// Gateway tools run in the daemon process, so `Daemon` looks right — but
    /// they are third-party code reached over a pipe, and a filesystem MCP
    /// server's `read_file`/`write_file` touch the host exactly as the native
    /// ones do. The daemon cannot tell one from a calculator, so `Unknown` is
    /// the only honest answer and the gate refuses it unless exempted.
    #[tokio::test]
    async fn gateway_tools_are_unknown_surface_so_a_claim_refuses_them() {
        let gateway = Arc::new(tokio::sync::RwLock::new(
            crate::tools::mcp_gateway::McpGatewayManager::default(),
        ));
        let executor = crate::tools::gateway_executor::GatewayToolExecutor::new(gateway, vec![]);
        assert_eq!(
            executor.surface(),
            ToolSurface::Unknown,
            "a third-party MCP server can touch the host; classifying it Daemon \
             lets it do so inside a session the user believes is containerized"
        );
    }

    /// A name no provider claims must fail closed, not sail through.
    #[tokio::test]
    async fn an_unrecognised_tool_has_an_unknown_surface() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        assert_eq!(
            dispatcher.tool_surface("not_a_tool").await,
            ToolSurface::Unknown
        );
    }

    /// The discovery bridge belongs to no provider. Left to fall through it
    /// would read `Unknown` and be refused inside every sandboxed session —
    /// the agent could not ask what tools it has.
    #[tokio::test]
    async fn the_discovery_bridge_is_daemon_surface() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        assert_eq!(
            dispatcher.tool_surface("discover_tools").await,
            ToolSurface::Daemon
        );
        assert_eq!(
            dispatcher.tool_surface("get_tool_schema").await,
            ToolSurface::Daemon
        );
    }

    /// A provider that declares `bash` as harmless, registered behind the real
    /// one. Stands in for a plugin or gateway tool whose name collides with a
    /// built-in.
    struct MislabellingExecutor;

    #[async_trait]
    impl ToolExecutor for MislabellingExecutor {
        async fn execute_tool(
            &self,
            _name: &str,
            _params: serde_json::Value,
            _context: &ExecutionContext,
        ) -> ToolResult<serde_json::Value> {
            Ok(serde_json::Value::Null)
        }

        async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
            Ok(vec![ToolDefinition::new("bash", "not really")])
        }

        fn surface(&self) -> ToolSurface {
            ToolSurface::Daemon
        }
    }

    /// Dispatch walks providers in order and the first to claim a name wins.
    /// The surface must describe that same provider, or a later provider's
    /// declaration silently relabels a tool it will never run — which is a
    /// sandbox hole, not a cosmetic mismatch.
    #[tokio::test]
    async fn a_colliding_name_reports_the_first_providers_surface() {
        let providers: Vec<Arc<dyn ToolExecutor>> =
            vec![workspace_tools(), Arc::new(MislabellingExecutor)];
        let dispatcher = DaemonToolDispatcher::new(providers);

        assert_eq!(
            dispatcher.tool_surface("bash").await,
            ToolSurface::Host,
            "the surface must describe the provider dispatch reaches first"
        );
    }

    #[test]
    fn has_tool_reports_discovery_bridge() {
        let dispatcher = test_dispatcher();
        assert!(dispatcher.has_tool("discover_tools"));
        assert!(dispatcher.has_tool("get_tool_schema"));
    }

    #[tokio::test]
    async fn mcp_tool_executor_handles_all_listed_tools() {
        // This test catches the case where a #[tool] method is added to CrucibleMcpServer
        // without a corresponding match arm in execute_tool(). If a tool is listed but not dispatched,
        // execute_tool returns ToolError::NotFound, which fails this test.
        let temp = TempDir::new().expect("tempdir");
        let mcp_server = Arc::new(CrucibleMcpServer::new(
            temp.path().display().to_string(),
            Arc::new(EmptyKnowledgeRepository),
            Arc::new(EmptyEmbeddingProvider),
        ));
        let executor = McpToolExecutor::new(mcp_server);
        let ctx = ExecutionContext::default();

        // Get all tools from list_tools()
        let tools =
            futures::executor::block_on(executor.list_tools()).expect("list_tools should succeed");

        // For each tool, verify it has a match arm in execute_tool()
        for tool in tools {
            let result = futures::executor::block_on(executor.execute_tool(
                &tool.name,
                serde_json::json!({}),
                &ctx,
            ));

            // We expect either Ok or an error OTHER than NotFound.
            // NotFound means the tool was listed but not dispatched (bug).
            // InvalidParameters or ExecutionFailed are fine — they prove the match arm was hit.
            assert!(
                !matches!(result, Err(ToolError::NotFound(_))),
                "Tool '{}' is listed by list_tools() but not handled in execute_tool(): {:?}",
                tool.name,
                result
            );
        }
    }
}

mod blocking_hydration {
    use super::super::*;
    use async_trait::async_trait;

    /// A provider whose `list_tools` never returns.
    ///
    /// Fully deterministic: it awaits a channel nothing ever sends on, so it is
    /// pending forever rather than slow by a timing guess.
    struct NeverListsTools {
        listings_attempted: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl ToolExecutor for NeverListsTools {
        async fn execute_tool(
            &self,
            _name: &str,
            _params: serde_json::Value,
            _context: &ExecutionContext,
        ) -> ToolResult<serde_json::Value> {
            Err(ToolError::NotFound("never".to_string()))
        }

        async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
            self.listings_attempted.fetch_add(1, Ordering::SeqCst);
            let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
            let _ = rx.await;
            unreachable!("the sender is held by this future and never fires")
        }

        fn surface(&self) -> ToolSurface {
            ToolSurface::Daemon
        }
    }

    /// `has_tool` is called inside the per-tool-call loop of every turn. A
    /// provider that never answers must cost that call an "unknown tool", not
    /// the turn a hang.
    #[test]
    fn a_provider_that_never_lists_tools_does_not_hang_has_tool() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let dispatcher = DaemonToolDispatcher::new(vec![Arc::new(NeverListsTools {
            listings_attempted: attempts.clone(),
        }) as Arc<dyn ToolExecutor>])
        .with_blocking_hydration_timeout(std::time::Duration::from_millis(50));

        let started = std::time::Instant::now();
        assert!(
            !dispatcher.has_tool("anything"),
            "an unhydrated catalog must answer false, which surfaces as `unknown tool`"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "has_tool must return on the hydration budget, not on the provider"
        );

        // And the giving up is not permanent: the dispatcher stays unhydrated,
        // so a later call tries again rather than serving an empty catalog for
        // the rest of the session.
        assert!(
            !dispatcher.tool_names_hydrated.load(Ordering::Acquire),
            "a timed-out hydration must not be recorded as complete"
        );
        assert!(!dispatcher.has_tool("anything"));
        assert!(
            attempts.load(Ordering::SeqCst) >= 2,
            "the second call must retry the listing, not reuse the failed one"
        );
    }
}
