//! Agent and skills RPC methods
//!
//! Methods for managing agents, skills, and models.
//!
//! Every method below keeps its own hand-written place because it does
//! real work the generated `rpc_<variant>` method does not: a retry
//! policy, a download timeout, an argument transform several callers
//! need, or a decode from a wire-only shape (a `String` enum tag, a
//! borrowed `&Path`) into the caller's own type. A method that only
//! built the row's request from its arguments and called the row — no
//! transform, no retry, no decode — is gone; its callers now call the
//! generated method directly (Simplification Plan step 19 item 9).

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;
use std::path::Path;
use std::time::Duration;

use super::DaemonClient;
use crucible_core::protocol::requests::Scoped;
use crucible_core::types::{KnobValue, SessionKnob};

impl DaemonClient {
    /// Serializes a typed `&SessionAgent` into the row's wire `Value` and
    /// discards the reply. 13 call sites configure a session's agent this
    /// way rather than building `AgentConfig` and handling the fallible
    /// serialization themselves.
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

    /// Write one knob's value. One RPC method for every knob, so a knob added
    /// later needs no sibling method here — the caller builds the
    /// [`KnobValue`] variant for the knob it wants to change.
    pub async fn session_knob_set(&self, session_id: &str, value: KnobValue) -> Result<()> {
        self.typed_unit_call(
            RpcMethod::SessionKnobSet,
            Scoped::new(session_id.to_string(), value),
        )
        .await
    }

    /// Read one knob's value, in the same [`KnobValue`] shape
    /// [`Self::session_knob_set`] writes. Retries on a transient failure,
    /// unlike the generated method's single attempt.
    pub async fn session_knob_get(&self, session_id: &str, knob: SessionKnob) -> Result<KnobValue> {
        self.call_with_retry(
            RpcMethod::SessionKnobGet,
            Scoped::new(session_id.to_string(), KnobRef { knob }),
        )
        .await
    }

    /// Set (Some) or detach (None) the session's workspace. Turns the
    /// caller's `Option<&Path>` into the row's `Option<String>`.
    pub async fn session_set_workspace(
        &self,
        session_id: &str,
        workspace: Option<&Path>,
    ) -> Result<serde_json::Value> {
        self.call(
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

    /// Turns a typed [`crucible_core::session::PluginApproval`] into the
    /// row's wire string and discards the reply. 9 call sites change a
    /// plugin's approval this way.
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

    /// Retries on a transient failure, and decodes the row's wire `String`
    /// back into [`crucible_core::session::PluginApproval`].
    pub async fn session_get_plugin_approval(
        &self,
        session_id: &str,
        plugin: &str,
    ) -> Result<crucible_core::session::PluginApproval> {
        let reply: PluginApprovalReply = self
            .call_with_retry(
                RpcMethod::SessionGetPluginApproval,
                Scoped::new(
                    session_id.to_owned(),
                    PluginRef {
                        plugin: plugin.to_owned(),
                    },
                ),
            )
            .await?;
        Ok(serde_json::from_value(serde_json::Value::String(
            reply.approval,
        ))?)
    }

    /// Retries on a transient failure, and decodes each wire `String` in
    /// the reply into [`crucible_core::session::PluginApproval`].
    pub async fn session_list_plugin_approvals(
        &self,
        session_id: &str,
    ) -> Result<std::collections::BTreeMap<String, crucible_core::session::PluginApproval>> {
        let reply: SessionListPluginApprovalsReply = self
            .call_with_retry(
                RpcMethod::SessionListPluginApprovals,
                Scoped::session(session_id.to_owned()),
            )
            .await?;
        Ok(reply.approvals)
    }

    /// Retries on a transient failure.
    pub async fn session_list_models(&self, session_id: &str) -> Result<Vec<String>> {
        let reply: SessionListModelsReply = self
            .call_with_retry(
                RpcMethod::SessionListModels,
                Scoped::session(session_id.to_string()),
            )
            .await?;
        Ok(reply.models)
    }

    /// The settings this session's external agent advertised for itself.
    /// Retries on a transient failure.
    pub async fn session_list_agent_options(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call_with_retry(
            RpcMethod::SessionListAgentOptions,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// Which settings this session can change. Retries on a transient
    /// failure.
    pub async fn session_list_knobs(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::types::SessionKnobSupport> {
        self.call_with_retry(
            RpcMethod::SessionListKnobs,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// The session's command catalog, in the order of its sources.
    /// Retries on a transient failure.
    pub async fn session_commands(
        &self,
        session_id: &str,
    ) -> Result<Vec<crucible_core::types::SessionCommand>> {
        let reply: SessionCommandsReply = self
            .call_with_retry(
                RpcMethod::SessionCommands,
                Scoped::session(session_id.to_string()),
            )
            .await?;
        Ok(reply.commands)
    }

    /// Retries on a transient failure.
    pub async fn session_list_modes(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::types::mode::SessionModes> {
        self.call_with_retry(
            RpcMethod::SessionListModes,
            Scoped::session(session_id.to_string()),
        )
        .await
    }

    /// List all available models without requiring an active session.
    /// Retries on a transient failure.
    ///
    /// If `kiln_path` is provided, the daemon resolves the kiln's data classification
    /// and filters providers whose trust level doesn't satisfy it.
    pub async fn list_all_models(&self, kiln_path: Option<&Path>) -> Result<Vec<String>> {
        let reply: ModelsListReply = self
            .call_with_retry(
                RpcMethod::ModelsList,
                ListAllModelsRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                },
            )
            .await?;
        Ok(reply.models)
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
            self.call_with_timeout(RpcMethod::EmbeddingsModels, params, DOWNLOAD_TIMEOUT)
                .await
        } else {
            self.call_with_retry(RpcMethod::EmbeddingsModels, params)
                .await
        }
    }

    /// Skips model discovery — no endpoint is dialed, so this returns fast
    /// even when a provider is down. The `models` field comes back empty
    /// and `available` is not meaningful; use this when only the
    /// *existence* of providers matters (preflight). Retries on a
    /// transient failure.
    ///
    /// `DaemonClient::list_providers` (`include_models` left `None`) had no
    /// caller and is gone; this is the only shape anything still asks for.
    pub async fn list_providers_summary(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        let reply: ProvidersListReply = self
            .call_with_retry(
                RpcMethod::ProvidersList,
                ListProvidersRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                    include_models: Some(false),
                },
            )
            .await?;
        Ok(reply.providers)
    }

    // =========================================================================
    // Skills Discovery RPC Methods
    // =========================================================================

    /// List discovered skills with optional scope filter. Turns `&Path`
    /// arguments into the row's wire `String` fields; the CLI
    /// (`crates/crucible-cli/src/commands/skills.rs`) calls this by name.
    pub async fn skills_list(
        &self,
        kiln_path: &Path,
        workspace: Option<&Path>,
        scope_filter: Option<&str>,
    ) -> Result<crucible_core::types::SkillsReply> {
        self.call(
            RpcMethod::SkillsList,
            SkillsListRequest {
                kiln_path: kiln_path.to_string_lossy().to_string(),
                workspace: workspace.map(|p| p.to_string_lossy().to_string()),
                scope_filter: scope_filter.map(|s| s.to_string()),
            },
        )
        .await
    }

    /// Get a single skill by name with full body. Turns `&Path` arguments
    /// into the row's wire `String` fields.
    pub async fn skills_get(
        &self,
        name: &str,
        kiln_path: &Path,
        workspace: Option<&Path>,
    ) -> Result<crucible_core::types::SkillDetail> {
        self.call(
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
    /// Turns `&Path` arguments into the row's wire `String` fields.
    pub async fn skills_search(
        &self,
        query: &str,
        kiln_path: &Path,
        workspace: Option<&Path>,
        limit: Option<usize>,
    ) -> Result<crucible_core::types::SkillsReply> {
        self.call(
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
}
