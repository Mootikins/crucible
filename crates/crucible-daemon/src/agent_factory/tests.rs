//! Tests for [`super`]: agent construction from a session config.
//!
//! A sibling file rather than an inline `mod tests`, for the file-size
//! gate — same shape as `kiln_manager/tests.rs`.

use super::*;

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::RwLock;

static OPENAI_API_KEY_LOCK: Mutex<()> = Mutex::new(());

async fn build_internal_tool_names_for_tests(
    workspace: &Path,
    kiln_path: Option<&Path>,
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    mcp_gateway: Option<Arc<RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    user_tools: &[McpToolInfo],
    mode: &str,
) -> Vec<String> {
    create_internal_mcp_tool_names_for_tests(
        workspace,
        kiln_path,
        mcp_gateway,
        &["gh".to_string()],
        knowledge_repo,
        embedding_provider,
        None,
        mode,
        Some(user_tools),
    )
    .await
}

fn test_agent_config() -> SessionAgent {
    SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: Some(0.7),
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: Default::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
    }
}

#[test]
fn enriched_prompt_carries_workspace_and_kiln_context() {
    let ws = Path::new("/repo");
    let kiln = Path::new("/repo/docs");

    // With kiln + base prompt: both paths present, base prompt before them.
    // The ordering is the reverse of what it once was — see
    // `the_cacheable_half_carries_nothing_session_specific` for why.
    let enriched = build_enriched_prompt(ws, Some(kiln), &[], "You are helpful.", "", "");
    let combined = enriched.combined();
    assert!(combined.contains("Workspace: /repo"));
    assert!(combined.contains("Kiln: /repo/docs"));
    assert!(combined.contains("You are helpful."));
    assert!(combined.find("You are helpful.").unwrap() < combined.find("Workspace:").unwrap());

    // Without kiln: no Kiln line
    let no_kiln = build_enriched_prompt(ws, None, &[], "Base.", "", "").combined();
    assert!(no_kiln.contains("Workspace: /repo"));
    assert!(!no_kiln.contains("Kiln:"));
    assert!(no_kiln.contains("Base."));

    // Empty base prompt: just context lines, no double blank
    let empty_base = build_enriched_prompt(ws, None, &[], "", "", "").combined();
    assert!(empty_base.contains("Workspace: /repo"));
    assert!(!empty_base.ends_with("\n\n"));

    // Skills catalog still follows the base prompt
    let with_skills = build_enriched_prompt(
        ws,
        Some(kiln),
        &[],
        "Base.",
        "",
        "# Available Skills\n\n## commit\n",
    )
    .combined();
    assert!(with_skills.contains("# Available Skills"));
    assert!(with_skills.find("Base.").unwrap() < with_skills.find("# Available Skills").unwrap());
}

/// The cached prefix must contain nothing that varies between sessions.
///
/// Prompt caching matches a token prefix. The workspace path was the first
/// line of the prompt, so two sessions in different projects diverged at byte
/// zero: the persona, `AGENTS.md` and the skills catalog were all re-ingested
/// for every project even though they were byte-identical.
#[test]
fn the_cacheable_half_carries_nothing_session_specific() {
    let prompt = build_enriched_prompt(
        Path::new("/repo"),
        Some(Path::new("/repo/docs")),
        &[],
        "You are helpful.",
        "# Project rules\n\nbe kind\n",
        "# Available Skills\n\n## commit\n",
    );

    assert!(prompt.stable.contains("You are helpful."));
    assert!(prompt.stable.contains("# Project rules"));
    assert!(prompt.stable.contains("# Available Skills"));
    assert!(
        !prompt.stable.contains("/repo"),
        "a session path in the cached prefix defeats reuse across projects:\n{}",
        prompt.stable
    );

    assert!(prompt.volatile.contains("Workspace: /repo"));
    assert!(prompt.volatile.contains("Kiln: /repo/docs"));
}

/// Least-stable last: the knowledge-base list names kilns, so it moves with
/// the session and belongs with the paths rather than with the persona.
#[test]
fn the_knowledge_base_list_is_session_context_not_cached_prefix() {
    let tmp = tempfile::TempDir::new().unwrap();
    let crucible_dir = tmp.path().join(".crucible");
    std::fs::create_dir_all(&crucible_dir).unwrap();
    std::fs::write(
        crucible_dir.join("kiln.toml"),
        "[kiln]\nname = \"My Kiln\"\n",
    )
    .unwrap();

    let prompt = build_enriched_prompt(
        Path::new("/workspace"),
        Some(tmp.path()),
        &[],
        "base",
        "",
        "",
    );

    assert!(prompt.volatile.contains("Knowledge bases:"));
    assert!(prompt.volatile.contains("My Kiln (primary)"));
    assert!(!prompt.stable.contains("Knowledge bases:"));
}

#[test]
fn build_enriched_prompt_includes_kiln_names() {
    let tmp = tempfile::TempDir::new().unwrap();
    let crucible_dir = tmp.path().join(".crucible");
    std::fs::create_dir_all(&crucible_dir).unwrap();
    std::fs::write(
        crucible_dir.join("kiln.toml"),
        "[kiln]\nname = \"My Kiln\"\n",
    )
    .unwrap();

    let result = build_enriched_prompt(
        Path::new("/workspace"),
        Some(tmp.path()),
        &[],
        "base",
        "",
        "",
    )
    .combined();
    assert!(
        result.contains("Knowledge bases:"),
        "should have kb section"
    );
    assert!(
        result.contains("My Kiln (primary)"),
        "should list primary kiln"
    );
}

#[test]
fn build_enriched_prompt_no_kiln_names_when_no_config() {
    let result =
        build_enriched_prompt(Path::new("/workspace"), None, &[], "base", "", "").combined();
    assert!(
        !result.contains("Knowledge bases:"),
        "no kb section when no kiln"
    );
}

#[test]
fn test_unsupported_agent_type() {
    let mut config = test_agent_config();
    config.agent_type = "unknown".to_string();

    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        create_agent_from_session_config(CreateAgentFromSessionConfigParams {
            modes: None,
            agent_config: &config,
            lua: None,
            workspace: Path::new("/tmp"),
            kiln_path: None,
            connected_kilns: &[],
            parent_session_id: None,
            background_spawner: None,
            delegation_spawner: None,
            mcp_gateway: None,
            acp_permission_handler: None,
            acp_config: None,
            context_config: None,
            knowledge_repo: None,
            embedding_provider: None,
            plugin_tools: None,
            sandbox_exec: None,
        })
        .await
    });

    assert!(matches!(
        result,
        Err(AgentFactoryError::UnsupportedAgentType(_))
    ));
}

#[tokio::test]
async fn internal_tools_include_adapter_tools() {
    let gateway = Arc::new(RwLock::new(
        crate::tools::mcp_gateway::McpGatewayManager::new(),
    ));

    let names = build_internal_tool_names_for_tests(
        Path::new("/tmp"),
        Some(Path::new("/tmp")),
        None,
        None,
        Some(gateway),
        &[],
        "auto",
    )
    .await;

    assert!(names.iter().any(|name| name == "semantic_search"));
    // delegate_session is filtered out when no delegation context is provided
    assert!(!names.iter().any(|name| name == "delegate_session"));
    assert!(names.iter().any(|name| name == "list_jobs"));
}

fn many_gateway_tools(n: usize) -> Vec<McpToolInfo> {
    (0..n)
        .map(|i| McpToolInfo {
            name: format!("tool_{i}"),
            prefixed_name: format!("gh_tool_{i}"),
            description: Some("a gateway tool with a description".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"q": {"type": "string", "description": "a query"}}
            }),
            upstream: "gh".to_string(),
            read_only: None,
        })
        .collect()
}

/// End-to-end through the factory: an over-budget agent attaches core tools
/// plus the three bridge defs and drops every gateway def; the plan-mode
/// variant attaches no gateway defs at all.
#[tokio::test]
async fn over_budget_agent_attaches_core_plus_bridge_and_plan_excludes_gateway() {
    use crucible_core::traits::chat::AgentHandle;

    let gateway_tools = many_gateway_tools(12);
    let gateway = Arc::new(RwLock::new(
        crate::tools::mcp_gateway::McpGatewayManager::new(),
    ));
    let (defs, deferrable, _plugin_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: None,
            workspace: Path::new("/tmp"),
            kiln_path: Some(Path::new("/tmp")),
            mcp_gateway: Some(gateway),
            server_names: &["gh".to_string()],
            knowledge_repo: None,
            embedding_provider: None,
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: Some(&gateway_tools),
            tool_policy: None,
            plugin_tools: None,
        })
        .await;
    assert_eq!(deferrable.len(), 12, "all gateway tools are deferrable");

    let config = LlmProviderConfig::builder(BackendType::OpenAI)
        .model("gpt-4o-mini")
        .build();
    let chat_client = ChatClient::new(&config);
    let client = chat_client.inner().clone();
    let model = chat_client
        .model_iden("gpt-4o-mini")
        .expect("model iden for gpt-4o-mini");
    let mut handle = GenaiAgentHandle::new(client, model, "system", defs, None)
        .with_deferrable_tools(deferrable);
    // Tiny budget → the tool schemas exceed the 15% share.
    handle.set_context_budget(Some(1_000)).await.unwrap();

    let (names, deferred) = handle.visible_tool_names_for_test();
    assert_eq!(deferred, 12, "every gateway tool deferred");
    assert!(names.iter().any(|n| n == "discover_tools"));
    assert!(names.iter().any(|n| n == "get_tool_schema"));
    assert!(names.iter().any(|n| n == "invoke_tool"));
    assert!(
        !names.iter().any(|n| n.starts_with("gh_")),
        "no gateway defs attached natively: {names:?}"
    );
    // Core kiln + workspace tools remain attached.
    assert!(names.iter().any(|n| n == "semantic_search"));
    assert!(names.iter().any(|n| n == "read_file"));

    // Plan-mode variant: gateway defs excluded categorically.
    handle.set_mode_str("plan").await.unwrap();
    let (plan_names, _) = handle.visible_tool_names_for_test();
    assert!(
        !plan_names.iter().any(|n| n.starts_with("gh_")),
        "no gateway defs in plan mode: {plan_names:?}"
    );
}

#[tokio::test]
async fn adapter_tools_come_before_user_mcp_tools() {
    let gateway = Arc::new(RwLock::new(
        crate::tools::mcp_gateway::McpGatewayManager::new(),
    ));

    let user_tools = vec![McpToolInfo {
        name: "search_repos".to_string(),
        prefixed_name: "gh_search_repos".to_string(),
        description: Some("Search repos".to_string()),
        input_schema: serde_json::json!({"type": "object"}),
        upstream: "gh".to_string(),
        read_only: None,
    }];

    let names = build_internal_tool_names_for_tests(
        Path::new("/tmp"),
        Some(Path::new("/tmp")),
        None,
        None,
        Some(gateway),
        &user_tools,
        "auto",
    )
    .await;

    let adapter_idx = names
        .iter()
        .position(|name| name == "semantic_search")
        .expect("semantic_search tool missing");
    let user_idx = names
        .iter()
        .position(|name| name == "gh_search_repos")
        .expect("user MCP tool missing");

    assert!(adapter_idx < user_idx);
}

/// A project rules file must reach the prompt the agent is built with.
///
/// Asserted through `create_agent_from_session_config` — the call
/// `send_message` makes — rather than on `build_enriched_prompt`, because
/// the way this feature died was the composition staying correct while
/// nothing ever called it with any rules. A test on the pure builder would
/// have passed throughout.
#[tokio::test]
async fn rules_file_contents_reach_the_system_prompt() {
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(
        ws.path().join("AGENTS.md"),
        "# House rules\n\nSENTINEL-RULES: always run `just ci` before committing.\n",
    )
    .unwrap();

    let config = test_agent_config();
    let handle = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
        modes: None,
        agent_config: &config,
        lua: None,
        workspace: ws.path(),
        kiln_path: None,
        connected_kilns: &[],
        parent_session_id: None,
        background_spawner: None,
        delegation_spawner: None,
        mcp_gateway: None,
        acp_permission_handler: None,
        acp_config: None,
        context_config: None,
        knowledge_repo: None,
        embedding_provider: None,
        plugin_tools: None,
        sandbox_exec: None,
    })
    .await
    .expect("agent creation should succeed");

    let prompt = handle
        .get_system_prompt()
        .expect("the genai handle knows the prompt it was built with");

    assert!(
        prompt.contains("SENTINEL-RULES"),
        "AGENTS.md contents must reach the system prompt, got: {prompt}"
    );
    // Layering: rules compose with the agent card's prompt rather than
    // replacing it, and the card speaks first.
    assert!(
        prompt.contains("You are a helpful assistant."),
        "the agent card's own prompt must survive, got: {prompt}"
    );
    assert!(
        prompt.find("You are a helpful assistant.").unwrap()
            < prompt.find("SENTINEL-RULES").unwrap(),
        "the agent card's prompt comes first, project rules refine it: {prompt}"
    );
}

/// Everything the session decided about generation and context must reach
/// the handle that talks to the model.
///
/// Asserted on the factory rather than on the setters, because the setters
/// were never the broken part: `session.set_temperature`, `cru.defaults`,
/// `[llm] temperature` and an agent card's `temperature:` all write to
/// `SessionAgent` correctly, and every one of those writes invalidates the
/// agent cache so the handle is rebuilt *here*. This is the single hop
/// where they were dropped.
#[tokio::test]
async fn session_generation_and_context_settings_reach_the_agent_handle() {
    let ws = tempfile::tempdir().unwrap();

    let config = SessionAgent {
        temperature: Some(0.2),
        max_tokens: Some(512),
        context_budget: Some(64_000),
        // Deliberately not `Truncate`: that is the default, so a handle
        // built with a default strategy would satisfy the assertion
        // without ever having read the session's choice.
        context_strategy: crucible_core::session::ContextStrategy::SlidingWindow,
        context_window: Some(128_000),
        ..test_agent_config()
    };

    let handle = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
        modes: None,
        agent_config: &config,
        lua: None,
        workspace: ws.path(),
        kiln_path: None,
        connected_kilns: &[],
        parent_session_id: None,
        background_spawner: None,
        delegation_spawner: None,
        mcp_gateway: None,
        acp_permission_handler: None,
        acp_config: None,
        context_config: None,
        knowledge_repo: None,
        embedding_provider: None,
        plugin_tools: None,
        sandbox_exec: None,
    })
    .await
    .expect("agent creation should succeed");

    assert_eq!(handle.get_temperature(), Some(0.2), "temperature");
    assert_eq!(handle.get_max_tokens(), Some(512), "max_tokens");
    assert_eq!(handle.get_context_budget(), Some(64_000), "context_budget");
    assert_eq!(
        handle.get_context_strategy(),
        crucible_core::session::ContextStrategy::SlidingWindow,
        "context_strategy"
    );
    assert_eq!(handle.get_context_window(), Some(128_000), "context_window");
}

#[tokio::test]
#[ignore = "Requires Ollama to be running"]
async fn test_create_ollama_agent() {
    let config = test_agent_config();
    let result = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
        modes: None,
        agent_config: &config,
        lua: None,
        workspace: Path::new("/tmp"),
        kiln_path: None,
        connected_kilns: &[],
        parent_session_id: None,
        background_spawner: None,
        delegation_spawner: None,
        mcp_gateway: None,
        acp_permission_handler: None,
        acp_config: None,
        context_config: None,
        knowledge_repo: None,
        embedding_provider: None,
        plugin_tools: None,
        sandbox_exec: None,
    })
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn internal_agent_type_dispatches_to_internal_branch() {
    // Verify that agent_type == "internal" takes the internal creation path
    // (not the ACP path). This test validates the dispatch logic by checking
    // that the function successfully creates an agent handle for internal agents.
    let config = test_agent_config();
    assert_eq!(config.agent_type, "internal");

    let result = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
        modes: None,
        agent_config: &config,
        lua: None,
        workspace: Path::new("/tmp"),
        kiln_path: None,
        connected_kilns: &[],
        parent_session_id: None,
        background_spawner: None,
        delegation_spawner: None,
        mcp_gateway: None,
        acp_permission_handler: None,
        acp_config: None,
        context_config: None,
        knowledge_repo: None,
        embedding_provider: None,
        plugin_tools: None,
        sandbox_exec: None,
    })
    .await;

    // The internal branch should succeed in creating an agent handle.
    // (Ollama client creation doesn't validate connectivity, just creates the object.)
    assert!(result.is_ok(), "Internal agent creation should succeed");
}

#[tokio::test]
async fn acp_agent_type_dispatches_to_acp_branch() {
    // Verify that agent_type == "acp" takes the ACP creation path
    // (not the internal path). This test validates the dispatch logic.
    let mut config = test_agent_config();
    config.agent_type = "acp".to_string();

    let result = create_agent_from_session_config(CreateAgentFromSessionConfigParams {
        modes: None,
        agent_config: &config,
        lua: None,
        workspace: Path::new("/tmp"),
        kiln_path: None,
        connected_kilns: &[],
        parent_session_id: None,
        background_spawner: None,
        delegation_spawner: None,
        mcp_gateway: None,
        acp_permission_handler: None,
        acp_config: None,
        context_config: None,
        knowledge_repo: None,
        embedding_provider: None,
        plugin_tools: None,
        sandbox_exec: None,
    })
    .await;

    // The result will be an error because ACP agent creation requires
    // proper ACP config and spawner setup, but it should be an AgentBuild error
    // (from the ACP branch), not an UnsupportedAgentType error.
    match result {
        Err(AgentFactoryError::AgentBuild(_)) => {
            // Expected: ACP branch was taken and failed during ACP agent creation
        }
        Err(AgentFactoryError::UnsupportedAgentType(_)) => {
            panic!("Should not reach UnsupportedAgentType for 'acp' agent type");
        }
        Ok(_) => {
            panic!("Should fail without proper ACP config");
        }
        Err(AgentFactoryError::ClientCreation(_)) => {
            panic!("Should not reach ClientCreation for ACP agent type");
        }
    }
}

#[test]
fn lua_auth_headers_override_config_when_authorization_present() {
    let _env_lock = OPENAI_API_KEY_LOCK
        .lock()
        .expect("OPENAI_API_KEY_LOCK should not be poisoned");
    let _guard =
        crucible_core::test_support::EnvVarGuard::set("OPENAI_API_KEY", "config-key".to_string());

    let lua = Lua::new();
    let globals = lua.globals();
    let crucible = lua.create_table().unwrap();
    globals.set("crucible", crucible.clone()).unwrap();
    crucible_lua::auth_plugin::register_auth_module(&lua, &crucible).unwrap();
    lua.load(
        r#"
        crucible.on_provider_auth(function(ctx)
            if ctx.provider == "openai" then
                return {
                    headers = {
                        ["Authorization"] = "Bearer lua-key"
                    }
                }
            end
            return nil
        end)
        "#,
    )
    .exec()
    .unwrap();

    let hooks = get_provider_auth_hooks(&lua).unwrap();
    let auth_headers = fire_provider_auth_hooks(&lua, &hooks, "openai", "gpt-4o")
        .unwrap()
        .unwrap();
    let from_lua = auth_headers.get("Authorization").unwrap();
    let selected = from_lua.strip_prefix("Bearer ").unwrap_or(from_lua);

    assert_eq!(selected, "lua-key");
}

#[test]
fn lua_auth_none_keeps_config_fallback() {
    let _env_lock = OPENAI_API_KEY_LOCK
        .lock()
        .expect("OPENAI_API_KEY_LOCK should not be poisoned");
    let _guard =
        crucible_core::test_support::EnvVarGuard::set("OPENAI_API_KEY", "config-key".to_string());

    let lua = Lua::new();
    let globals = lua.globals();
    let crucible = lua.create_table().unwrap();
    globals.set("crucible", crucible.clone()).unwrap();
    crucible_lua::auth_plugin::register_auth_module(&lua, &crucible).unwrap();
    lua.load(
        r#"
        crucible.on_provider_auth(function(_ctx)
            return nil
        end)
        "#,
    )
    .exec()
    .unwrap();

    let hooks = get_provider_auth_hooks(&lua).unwrap();
    let auth_headers = fire_provider_auth_hooks(&lua, &hooks, "openai", "gpt-4o").unwrap();

    assert!(auth_headers.is_none());
}

#[tokio::test]
async fn test_tool_definitions_include_get_kiln_info() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let kiln_path = temp_dir.path();

    let knowledge_repo: Arc<dyn KnowledgeRepository> = Arc::new(EmptyKnowledgeRepository);
    let embedding_provider: Arc<dyn EmbeddingProvider> = Arc::new(EmptyEmbeddingProvider);

    let (tools, _deferrable, _plugin_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: None,
            workspace: Path::new("/tmp"),
            kiln_path: Some(kiln_path),
            mcp_gateway: None,
            server_names: &[],
            knowledge_repo: Some(knowledge_repo),
            embedding_provider: Some(embedding_provider),
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: None,
            tool_policy: None,
            plugin_tools: None,
        })
        .await;

    let get_kiln_info_tool = tools
        .iter()
        .find(|t| t.function.name == "get_kiln_info")
        .expect("get_kiln_info tool should exist in in-process tools");
    assert!(!get_kiln_info_tool.function.description.is_empty());
}

/// A registry holding one plugin tool named `plugin_echo`.
fn registry_with_one_plugin_tool() -> (mlua::Lua, Arc<crate::plugin_tools::PluginRegistry>) {
    let lua = mlua::Lua::new();
    let func: mlua::Function = lua
        .load("return function(args) return args end")
        .eval()
        .expect("eval fn");
    let registry = Arc::new(crate::plugin_tools::PluginRegistry::new());
    registry.register_plugin(
        "fixture",
        &lua,
        &[crucible_lua::DiscoveredTool {
            name: "plugin_echo".to_string(),
            description: "Echo the input".to_string(),
            params: vec![crucible_lua::DiscoveredParam {
                name: "text".to_string(),
                param_type: "string".to_string(),
                description: "Text".to_string(),
                optional: false,
            }],
            return_type: None,
            source_path: "fixture".to_string(),
            is_fennel: false,
        }],
        &[],
        std::collections::HashMap::from([("plugin_echo".to_string(), func)]),
        std::collections::HashMap::new(),
    );
    (lua, registry)
}

#[tokio::test]
async fn plugin_tools_are_advertised_to_the_model() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let (_lua, registry) = registry_with_one_plugin_tool();

    let (tools, deferrable, _plugin_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: None,
            workspace: Path::new("/tmp"),
            kiln_path: Some(temp_dir.path()),
            mcp_gateway: None,
            server_names: &[],
            knowledge_repo: None,
            embedding_provider: None,
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: None,
            tool_policy: None,
            plugin_tools: Some(registry),
        })
        .await;

    let echo = tools
        .iter()
        .find(|t| t.function.name == "plugin_echo")
        .expect("plugin tool should be advertised to the model");
    assert_eq!(echo.function.description, "Echo the input");
    let schema = echo
        .function
        .parameters
        .as_ref()
        .expect("plugin tool should carry a parameter schema");
    assert_eq!(
        schema["properties"]["text"]["type"], "string",
        "spec params should become a JSON schema: {schema:?}"
    );
    assert!(
        !deferrable.contains("plugin_echo"),
        "plugin tools are not deferrable"
    );
}

/// Plugin defs are attached in EVERY mode and their names returned, so
/// the per-request filter (`visible_tools`) and the dispatch guard own
/// plan-mode exclusion. Skipping them at creation looked equivalent but
/// wasn't: mode is captured when the agent is built, so a session
/// switched to plan mid-run kept its plugin tools advertised — and one
/// created in plan could never gain them back in act.
#[tokio::test]
async fn plugin_tools_are_attached_in_plan_mode_and_reported_for_filtering() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let (_lua, registry) = registry_with_one_plugin_tool();

    let (tools, _deferrable, plugin_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: None,
            workspace: Path::new("/tmp"),
            kiln_path: Some(temp_dir.path()),
            mcp_gateway: None,
            server_names: &[],
            knowledge_repo: None,
            embedding_provider: None,
            delegation_context: None,
            mode: "plan",
            gateway_all_tools_override: None,
            tool_policy: None,
            plugin_tools: Some(registry),
        })
        .await;

    assert!(
        tools.iter().any(|t| t.function.name == "plugin_echo"),
        "plugin defs attach in every mode; visible_tools() filters per request"
    );
    assert!(
        plugin_names.contains("plugin_echo"),
        "the name set is what lets the runtime filter and dispatch guard act"
    );
}

#[tokio::test]
async fn workspace_tools_in_agent_tool_defs() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let kiln_path = temp_dir.path();

    let knowledge_repo: Arc<dyn KnowledgeRepository> = Arc::new(EmptyKnowledgeRepository);
    let embedding_provider: Arc<dyn EmbeddingProvider> = Arc::new(EmptyEmbeddingProvider);

    let (tools, _deferrable, _plugin_names) =
        create_internal_mcp_tool_defs(CreateInternalMcpToolDefsParams {
            modes: None,
            workspace: Path::new("/tmp"),
            kiln_path: Some(kiln_path),
            mcp_gateway: None,
            server_names: &[],
            knowledge_repo: Some(knowledge_repo),
            embedding_provider: Some(embedding_provider),
            delegation_context: None,
            mode: "auto",
            gateway_all_tools_override: None,
            tool_policy: None,
            plugin_tools: None,
        })
        .await;

    let tool_names: Vec<String> = tools.iter().map(|t| t.function.name.clone()).collect();

    // These assertions FAIL because workspace tools are not yet included
    assert!(
        tool_names.iter().any(|name| name == "bash"),
        "bash tool should be in agent tool defs"
    );
    assert!(
        tool_names.iter().any(|name| name == "read_file"),
        "read_file tool should be in agent tool defs"
    );
    assert!(
        tool_names.iter().any(|name| name == "edit_file"),
        "edit_file tool should be in agent tool defs"
    );
    assert!(
        tool_names.iter().any(|name| name == "write_file"),
        "write_file tool should be in agent tool defs"
    );
    assert!(
        tool_names.iter().any(|name| name == "glob"),
        "glob tool should be in agent tool defs"
    );
    assert!(
        tool_names.iter().any(|name| name == "grep"),
        "grep tool should be in agent tool defs"
    );
}

#[test]
fn is_safe_classifies_workspace_tools() {
    use crate::agent_manager::is_safe;

    // These assertions test the current state of is_safe()
    // Some may FAIL if is_safe() doesn't have these tool names yet
    assert!(
        !is_safe("bash"),
        "bash should be unsafe (runs arbitrary commands)"
    );
    assert!(is_safe("read_file"), "read_file should be safe (read-only)");
    assert!(
        !is_safe("write_file"),
        "write_file should be unsafe (modifies files)"
    );
    assert!(
        !is_safe("edit_file"),
        "edit_file should be unsafe (modifies files)"
    );
    assert!(is_safe("glob"), "glob should be safe (read-only)");
    assert!(is_safe("grep"), "grep should be safe (read-only)");
}

/// No tool may be advertised to the model twice.
///
/// A kiln-backed session took its tool definitions from two sources that
/// overlap: `CrucibleMcpServer::list_tools()` and
/// `WorkspaceTools::tool_definitions()`. Both carry `read_file`, `edit_file`,
/// `write_file`, `bash`, `glob` and `grep`, and nothing deduplicates, so every
/// kiln session paid for six duplicate schemas and handed the provider a
/// function list with repeated names.
#[tokio::test]
async fn no_tool_is_advertised_to_the_model_twice() {
    let tmp = tempfile::tempdir().unwrap();
    let names = create_internal_mcp_tool_names_for_tests(
        tmp.path(),
        Some(tmp.path()),
        None,
        &[],
        None,
        None,
        None,
        "normal",
        None,
    )
    .await;

    let mut seen = std::collections::HashSet::new();
    let dupes: Vec<&String> = names.iter().filter(|n| !seen.insert(*n)).collect();

    assert!(
        dupes.is_empty(),
        "these tools are advertised more than once: {dupes:?} (full list: {names:?})"
    );
}
