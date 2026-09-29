//! Agent and skills RPC methods
//!
//! Methods for managing agents, skills, and models.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;
use std::path::Path;
use std::time::Duration;

use super::types::extract_string_array;
use super::{DaemonClient, NO_PARAMS};
use crucible_core::protocol::requests::NameRequest;
use crucible_core::protocol::requests::Scoped;

impl DaemonClient {
    pub async fn session_configure_agent(
        &self,
        session_id: &str,
        agent: &crucible_core::session::SessionAgent,
    ) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionConfigureAgent,
            Scoped::new(
                session_id.to_string(),
                AgentConfig {
                    agent: serde_json::to_value(agent)?,
                },
            ),
        )
        .await
    }

    pub async fn session_switch_model(&self, session_id: &str, model_id: &str) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSwitchModel,
            SessionSwitchModelRequest {
                session_id: session_id.to_string(),
                model_id: model_id.to_string(),
            },
        )
        .await
    }

    /// Attach a kiln to a session's set. Returns the updated scope
    /// `{session_id, kilns, workspace}`.
    pub async fn session_connect_kiln(
        &self,
        session_id: &str,
        kiln: &crucible_core::config::KilnName,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionConnectKiln,
            Scoped::new(session_id.to_string(), NamedKiln { kiln: kiln.clone() }),
        )
        .await
    }

    /// Detach a kiln from the session's set. Any member may be detached — the
    /// set is flat, including the kiln the session was created with.
    pub async fn session_disconnect_kiln(
        &self,
        session_id: &str,
        kiln: &crucible_core::config::KilnName,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionDisconnectKiln,
            Scoped::new(session_id.to_string(), NamedKiln { kiln: kiln.clone() }),
        )
        .await
    }

    /// Set (Some) or detach (None) the session's workspace.
    pub async fn session_set_workspace(
        &self,
        session_id: &str,
        workspace: Option<&Path>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::SessionSetWorkspace,
            Scoped::new(
                session_id.to_string(),
                WorkspaceChoice {
                    workspace: workspace.map(|p| p.to_string_lossy().to_string()),
                },
            ),
        )
        .await
    }

    pub async fn session_set_mode(&self, session_id: &str, mode_id: &str) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSetMode,
            SessionSetModeRequest {
                session_id: session_id.to_string(),
                mode_id: mode_id.to_string(),
            },
        )
        .await
    }

    pub async fn session_set_plugin_approval(
        &self,
        session_id: &str,
        plugin: &str,
        approval: crucible_core::session::PluginApproval,
    ) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSetPluginApproval,
            Scoped::new(
                session_id.to_owned(),
                PluginApprovalChange {
                    plugin: plugin.to_owned(),
                    approval: approval.as_str().to_owned(),
                },
            ),
        )
        .await
    }

    pub async fn session_set_plugin_turn_limit(&self, session_id: &str, limit: u32) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSetPluginTurnLimit,
            SessionPluginTurnLimitRequest {
                session_id: session_id.to_owned(),
                limit,
            },
        )
        .await
    }

    pub async fn session_get_plugin_turn_limit(&self, session_id: &str) -> Result<u32> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::SessionGetPluginTurnLimit,
                Scoped::session(session_id.to_string()),
            )
            .await?;
        let limit = result
            .get("limit")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("session.get_plugin_turn_limit omitted limit"))?;
        Ok(u32::try_from(limit)?)
    }

    pub async fn session_get_plugin_approval(
        &self,
        session_id: &str,
        plugin: &str,
    ) -> Result<crucible_core::session::PluginApproval> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::SessionGetPluginApproval,
                Scoped::new(
                    session_id.to_owned(),
                    PluginRef {
                        plugin: plugin.to_owned(),
                    },
                ),
            )
            .await?;
        Ok(serde_json::from_value(result["approval"].clone())?)
    }

    pub async fn session_list_plugin_approvals(
        &self,
        session_id: &str,
    ) -> Result<std::collections::BTreeMap<String, crucible_core::session::PluginApproval>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::SessionListPluginApprovals,
                Scoped::session(session_id.to_owned()),
            )
            .await?;
        Ok(serde_json::from_value(result["approvals"].clone())?)
    }

    pub async fn session_list_models(&self, session_id: &str) -> Result<Vec<String>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::SessionListModels,
                Scoped::session(session_id.to_string()),
            )
            .await?;

        Ok(extract_string_array(&result, "models"))
    }

    /// The modes this session may enter, and the one it is in.
    ///
    /// The list is per-session because it is resolved from the session's Lua
    /// registry — two sessions in different projects can offer different modes.
    /// The settings this session's external agent advertised for itself.
    pub async fn session_list_agent_options(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call_with_retry(
            RpcMethod::SessionListAgentOptions,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// Set one of the agent's own settings.
    pub async fn session_set_agent_option(
        &self,
        session_id: &str,
        option_id: &str,
        value: &str,
    ) -> Result<()> {
        #[derive(serde::Serialize)]
        struct Params<'a> {
            session_id: &'a str,
            option_id: &'a str,
            value: &'a str,
        }
        let _: serde_json::Value = self
            .typed_call(
                RpcMethod::SessionSetAgentOption,
                Params {
                    session_id,
                    option_id,
                    value,
                },
            )
            .await?;
        Ok(())
    }

    /// Which settings this session can change.
    pub async fn session_list_knobs(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::types::SessionKnobSupport> {
        self.typed_call_with_retry(
            RpcMethod::SessionListKnobs,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// The session's command catalog, in the order of its sources.
    pub async fn session_commands(
        &self,
        session_id: &str,
    ) -> Result<Vec<crucible_core::types::SessionCommand>> {
        #[derive(serde::Deserialize)]
        struct Reply {
            commands: Vec<crucible_core::types::SessionCommand>,
        }
        let reply: Reply = self
            .typed_call_with_retry(
                RpcMethod::SessionCommands,
                Scoped::session(session_id.to_string()),
            )
            .await?;
        Ok(reply.commands)
    }

    pub async fn session_list_modes(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::types::mode::SessionModes> {
        self.typed_call_with_retry(
            RpcMethod::SessionListModes,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// List all available models without requiring an active session.
    ///
    /// If `kiln_path` is provided, the daemon resolves the kiln's data classification
    /// and filters providers whose trust level doesn't satisfy it.
    pub async fn list_all_models(&self, kiln_path: Option<&Path>) -> Result<Vec<String>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::ModelsList,
                ListAllModelsRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                },
            )
            .await?;

        Ok(extract_string_array(&result, "models"))
    }

    /// The local embedding catalog, and what of it is already on disk.
    ///
    /// `model` names one model to resolve through the catalog; `download`
    /// fetches it first. A fetch reads a few hundred megabytes over the
    /// network, so the timeout is the download's, not the default request's.
    pub async fn embedding_models(
        &self,
        model: Option<&str>,
        download: bool,
    ) -> Result<EmbeddingCatalog> {
        /// Long enough for the largest model on a slow link.
        const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(1800);

        let params = EmbeddingModelsRequest {
            model: model.map(str::to_string),
            download,
        };
        if download {
            self.typed_call_with_timeout(RpcMethod::EmbeddingsModels, params, DOWNLOAD_TIMEOUT)
                .await
        } else {
            self.typed_call_with_retry(RpcMethod::EmbeddingsModels, params)
                .await
        }
    }

    /// List all available providers without requiring an active session.
    pub async fn list_providers(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        self.list_providers_inner(kiln_path, None).await
    }

    /// Like [`Self::list_providers`], but skips model discovery — no endpoint
    /// is dialed, so this returns fast even when a provider is down. The
    /// `models` field comes back empty and `available` is not meaningful;
    /// use this when only the *existence* of providers matters (preflight).
    pub async fn list_providers_summary(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        self.list_providers_inner(kiln_path, Some(false)).await
    }

    async fn list_providers_inner(
        &self,
        kiln_path: Option<&std::path::Path>,
        include_models: Option<bool>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::ProvidersList,
                ListProvidersRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                    include_models,
                },
            )
            .await?;
        let providers = result["providers"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(providers)
    }

    /// Set whether Precognition (auto-RAG) is enabled for a session.
    pub async fn session_set_precognition(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSetPrecognition,
            SessionSetPrecognitionRequest {
                session_id: session_id.to_string(),
                precognition_enabled: enabled,
            },
        )
        .await
    }

    /// Get whether Precognition is enabled for a session.
    pub async fn session_get_precognition(&self, session_id: &str) -> Result<bool> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                RpcMethod::SessionGetPrecognition,
                Scoped::session(session_id.to_string()),
            )
            .await?;

        let enabled = result
            .get("precognition_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(enabled)
    }

    pub async fn session_get_mode(&self, session_id: &str) -> Result<Option<String>> {
        self.get_session_option(RpcMethod::SessionGetMode, session_id, "mode", |v| {
            v.as_str().map(|s| s.to_string())
        })
        .await
    }

    pub async fn session_set_context_strategy(
        &self,
        session_id: &str,
        strategy: &str,
    ) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionSetContextStrategy,
            SessionSetContextStrategyRequest {
                session_id: session_id.to_string(),
                context_strategy: strategy.to_string(),
            },
        )
        .await
    }

    pub async fn session_get_context_strategy(&self, session_id: &str) -> Result<Option<String>> {
        self.get_session_option(
            RpcMethod::SessionGetContextStrategy,
            session_id,
            "context_strategy",
            |v| v.as_str().map(String::from),
        )
        .await
    }

    /// Undo the last N agent turns for a session.
    pub async fn session_undo(
        &self,
        session_id: &str,
        count: usize,
    ) -> Result<Vec<crucible_core::types::UndoSummary>> {
        let resp: serde_json::Value = self
            .typed_call(
                RpcMethod::SessionUndo,
                Scoped::new(session_id.to_string(), UndoCount { count: Some(count) }),
            )
            .await?;
        let undone = resp
            .get("undone")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let summaries: Vec<crucible_core::types::UndoSummary> =
            serde_json::from_value(undone).unwrap_or_default();
        Ok(summaries)
    }

    // =========================================================================
    // Skills Discovery RPC Methods
    // =========================================================================

    /// List discovered skills with optional scope filter.
    pub async fn skills_list(
        &self,
        kiln_path: &Path,
        workspace: Option<&Path>,
        scope_filter: Option<&str>,
    ) -> Result<crucible_core::types::SkillsReply> {
        self.typed_call(
            RpcMethod::SkillsList,
            SkillsListRequest {
                kiln_path: kiln_path.to_string_lossy().to_string(),
                workspace: workspace.map(|p| p.to_string_lossy().to_string()),
                scope_filter: scope_filter.map(|s| s.to_string()),
            },
        )
        .await
    }

    /// Get a single skill by name with full body.
    pub async fn skills_get(
        &self,
        name: &str,
        kiln_path: &Path,
        workspace: Option<&Path>,
    ) -> Result<crucible_core::types::SkillDetail> {
        self.typed_call(
            RpcMethod::SkillsGet,
            SkillsGetRequest {
                name: name.to_string(),
                kiln_path: kiln_path.to_string_lossy().to_string(),
                workspace: workspace.map(|p| p.to_string_lossy().to_string()),
            },
        )
        .await
    }

    /// Search skills by text query (case-insensitive match on name + description).
    pub async fn skills_search(
        &self,
        query: &str,
        kiln_path: &Path,
        workspace: Option<&Path>,
        limit: Option<usize>,
    ) -> Result<crucible_core::types::SkillsReply> {
        self.typed_call(
            RpcMethod::SkillsSearch,
            SkillsSearchRequest {
                query: query.to_string(),
                kiln_path: kiln_path.to_string_lossy().to_string(),
                workspace: workspace.map(|p| p.to_string_lossy().to_string()),
                limit,
            },
        )
        .await
    }

    /// List all available agent profiles (builtins + configured).
    pub async fn agents_list_profiles(&self) -> Result<crate::AgentProfilesReply> {
        self.typed_call(RpcMethod::AgentsListProfiles, NO_PARAMS)
            .await
    }

    /// List the agent cards a session started from `workspace` would resolve.
    pub async fn agents_list_cards(
        &self,
        workspace: &Path,
        kiln_path: Option<&Path>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::AgentsListCards,
            AgentsListCardsRequest {
                workspace: workspace.to_string_lossy().to_string(),
                kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
            },
        )
        .await
    }

    /// Resolve a named agent profile.
    pub async fn agents_resolve_profile(&self, name: &str) -> Result<serde_json::Value> {
        self.typed_call(
            RpcMethod::AgentsResolveProfile,
            NameRequest {
                name: name.to_string(),
            },
        )
        .await
    }
}
