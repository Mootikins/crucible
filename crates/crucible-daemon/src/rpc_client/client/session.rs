//! Session RPC methods
//!
//! Methods for managing chat sessions, sending messages, and configuring agents.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::DaemonClient;

// =========================================================================
// Session RPC Request/Response Types
// =========================================================================

/// Request for `session.create`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionCreateRequest {
    #[serde(rename = "type")]
    pub session_type: String,
    /// Omitted → the daemon resolves its default (home kiln). Keeping the
    /// fallback daemon-side means clients can never drift from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiln: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_kilns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_path: Option<String>,
    /// "acp" | "internal"; None treated as "internal" for back-compat.
    /// Lets the daemon's setup task branch on agent type at create time,
    /// before `session.configure_agent` has been called.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,

    /// When true, the daemon resolves and configures the session's agent as
    /// part of create (ACP profile for `agent_type == "acp"`, otherwise
    /// config-derived internal defaults), and returns the resolved model in
    /// `agent_model`. Absent/false ⇒ today's behavior: the session is created
    /// agent-less and the caller configures it separately via
    /// `session.configure_agent`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub configure_agent: bool,
    /// ACP profile name; used when `configure_agent` and `agent_type == "acp"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Internal-agent overrides applied on top of config-derived defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Parameters for creating a session.
#[derive(Debug, Clone)]
pub struct SessionCreateParams {
    pub session_type: String,
    /// None → daemon default (home kiln).
    pub kiln: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub connect_kilns: Vec<PathBuf>,
    pub recording_mode: Option<String>,
    pub recording_path: Option<PathBuf>,
    /// "acp" | "internal"; None treated as "internal" for back-compat.
    pub agent_type: Option<String>,
}

/// Optional agent spec for `session.create` that asks the daemon to resolve and
/// configure the session's agent server-side (the "daemon owns defaults" path).
///
/// `agent_name` selects an ACP profile (with `agent_type == "acp"`); the
/// provider/model/endpoint fields override internal-agent config defaults. An
/// all-`None` spec on an internal session means "use the config defaults as-is".
#[derive(Debug, Clone, Default)]
pub struct SessionAgentSpec {
    pub agent_name: Option<String>,
    pub provider: Option<String>,
    pub provider_key: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
}

/// Build the wire request. `agent = Some(..)` sets `configure_agent = true` so
/// the daemon resolves + configures the agent as part of create; `None` keeps
/// the back-compat "create agent-less, configure later" shape.
fn build_create_request(
    params: SessionCreateParams,
    agent: Option<SessionAgentSpec>,
) -> SessionCreateRequest {
    let configure_agent = agent.is_some();
    let agent = agent.unwrap_or_default();
    SessionCreateRequest {
        session_type: params.session_type,
        kiln: params.kiln.map(|p| p.to_string_lossy().to_string()),
        workspace: params.workspace.map(|ws| ws.to_string_lossy().to_string()),
        connect_kilns: if params.connect_kilns.is_empty() {
            None
        } else {
            Some(
                params
                    .connect_kilns
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            )
        },
        recording_mode: params.recording_mode,
        recording_path: params
            .recording_path
            .map(|p| p.to_string_lossy().to_string()),
        agent_type: params.agent_type,
        configure_agent,
        agent_name: agent.agent_name,
        provider: agent.provider,
        provider_key: agent.provider_key,
        model: agent.model,
        endpoint: agent.endpoint,
    }
}

/// Request for `session.list`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionListRequest {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiln: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_archived: Option<bool>,
    /// Include delegated child sessions (hidden by default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_children: Option<bool>,
}

/// Shared request for methods that only require a `session_id`.
///
/// Used by: `session.get`, `session.pause`, `session.resume`, `session.end`,
/// `session.cancel`, `session.list_models`, `session.get_thinking_budget`,
/// `session.get_precognition`, `session.get_temperature`, `session.get_max_tokens`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionIdRequest {
    pub session_id: String,
}

/// Request for `session.replay`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionReplayRequest {
    pub recording_path: String,
    pub speed: f64,
}

/// Request for `session.resume_from_storage`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionResumeFromStorageRequest {
    pub session_id: String,
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDeleteRequest {
    pub session_id: String,
    pub kiln: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionArchiveRequest {
    pub session_id: String,
    pub kiln: String,
}

/// Request for `session.send_message`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSendMessageRequest {
    pub session_id: String,
    pub content: String,
    pub is_interactive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

/// Request for `session.interaction_respond`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInteractionRespondRequest {
    pub session_id: String,
    pub request_id: String,
    pub response: serde_json::Value,
}

/// Request for `session.set_title`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetTitleRequest {
    pub session_id: String,
    pub title: String,
}

/// Request for `session.search`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSearchRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiln: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Request for `session.load_events`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionLoadEventsRequest {
    pub session_dir: String,
}

/// Request for `session.list_persisted`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionListPersistedRequest {
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Request for `session.render_markdown`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionRenderMarkdownRequest {
    pub session_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timestamps: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tokens: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_length: Option<usize>,
}

/// Request for `session.export_to_file`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionExportToFileRequest {
    pub session_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timestamps: Option<bool>,
}

/// Request for `session.cleanup`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionCleanupRequest {
    pub kiln: String,
    pub older_than_days: u64,
    pub dry_run: bool,
}

/// Request for `session.reindex`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionReindexRequest {
    pub kiln: String,
    pub force: bool,
}

// --- Session RPC Response Types ---

/// Response from `session.send_message`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionSendMessageResponse {
    pub message_id: String,
}

/// Response from `session.cancel`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionCancelResponse {
    pub cancelled: bool,
}

/// Response from `session.render_markdown`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionRenderMarkdownResponse {
    pub markdown: String,
}

/// Response from `session.export_to_file`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionExportToFileResponse {
    pub output_path: String,
}

impl DaemonClient {
    // =========================================================================
    // Session RPC Methods
    // =========================================================================

    pub async fn session_create(&self, params: SessionCreateParams) -> Result<serde_json::Value> {
        self.typed_call("session.create", build_create_request(params, None))
            .await
    }

    /// Create a session AND have the daemon resolve + configure its agent in one
    /// call (the "daemon owns default-agent resolution" path). The response
    /// carries the resolved `agent_model`. An unknown ACP profile fails with
    /// `INVALID_PARAMS` and no session is created.
    pub async fn session_create_with_agent(
        &self,
        params: SessionCreateParams,
        agent: SessionAgentSpec,
    ) -> Result<serde_json::Value> {
        self.typed_call("session.create", build_create_request(params, Some(agent)))
            .await
    }

    pub async fn session_list(
        &self,
        kiln: Option<&Path>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
        include_archived: Option<bool>,
    ) -> Result<serde_json::Value> {
        self.session_list_with_children(kiln, workspace, session_type, state, include_archived, None)
            .await
    }

    /// `session.list` with explicit control over delegated-child visibility
    /// (children are hidden unless `include_children` is `Some(true)`).
    pub async fn session_list_with_children(
        &self,
        kiln: Option<&Path>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
        include_archived: Option<bool>,
        include_children: Option<bool>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.list",
            SessionListRequest {
                session_type: session_type.map(|t| t.to_string()),
                kiln: kiln.map(|k| k.to_string_lossy().to_string()),
                workspace: workspace.map(|ws| ws.to_string_lossy().to_string()),
                state: state.map(|s| s.to_string()),
                include_archived,
                include_children,
            },
        )
        .await
    }

    pub async fn session_get(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call("session.get", session_id).await
    }

    pub async fn session_pause(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call("session.pause", session_id).await
    }

    pub async fn session_resume(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call("session.resume", session_id).await
    }

    pub async fn session_end(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call("session.end", session_id).await
    }

    pub async fn session_delete(&self, session_id: &str, kiln: &Path) -> Result<serde_json::Value> {
        self.typed_call(
            "session.delete",
            SessionDeleteRequest {
                session_id: session_id.to_string(),
                kiln: kiln.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn session_archive(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.archive",
            SessionArchiveRequest {
                session_id: session_id.to_string(),
                kiln: kiln.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn session_unarchive(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.unarchive",
            SessionArchiveRequest {
                session_id: session_id.to_string(),
                kiln: kiln.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn session_replay(
        &self,
        recording_path: &Path,
        speed: f64,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.replay",
            SessionReplayRequest {
                recording_path: recording_path.to_string_lossy().to_string(),
                speed,
            },
        )
        .await
    }

    pub async fn session_resume_from_storage(
        &self,
        session_id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.resume_from_storage",
            SessionResumeFromStorageRequest {
                session_id: session_id.to_string(),
                kiln: kiln.to_string_lossy().to_string(),
                limit,
                offset,
            },
        )
        .await
    }

    pub async fn session_send_message(
        &self,
        session_id: &str,
        content: &str,
        is_interactive: bool,
    ) -> Result<String> {
        self.session_send_message_with_permissions(session_id, content, is_interactive, None)
            .await
    }

    pub async fn session_send_message_with_permissions(
        &self,
        session_id: &str,
        content: &str,
        is_interactive: bool,
        permission_mode: Option<String>,
    ) -> Result<String> {
        let resp: SessionSendMessageResponse = self
            .typed_call(
                "session.send_message",
                SessionSendMessageRequest {
                    session_id: session_id.to_string(),
                    content: content.to_string(),
                    is_interactive,
                    permission_mode,
                },
            )
            .await?;

        Ok(resp.message_id)
    }

    /// All pending interactions across sessions (`{pending: [{session_id,
    /// request_id, request}]}`) — polled by the web Inbox.
    pub async fn session_pending_interactions(&self) -> Result<serde_json::Value> {
        self.call("session.pending_interactions", serde_json::json!({}))
            .await
    }

    pub async fn session_interaction_respond(
        &self,
        session_id: &str,
        request_id: &str,
        response: crucible_core::interaction::InteractionResponse,
    ) -> Result<()> {
        self.typed_unit_call(
            "session.interaction_respond",
            SessionInteractionRespondRequest {
                session_id: session_id.to_string(),
                request_id: request_id.to_string(),
                response: serde_json::to_value(response)?,
            },
        )
        .await
    }

    pub async fn session_cancel(&self, session_id: &str) -> Result<bool> {
        let resp: SessionCancelResponse = self
            .typed_call(
                "session.cancel",
                SessionIdRequest {
                    session_id: session_id.to_string(),
                },
            )
            .await?;

        Ok(resp.cancelled)
    }

    pub async fn session_set_title(&self, session_id: &str, title: &str) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_title",
            SessionSetTitleRequest {
                session_id: session_id.to_string(),
                title: title.to_string(),
            },
        )
        .await
    }

    /// Generate a topic-based title for a session (idempotent — returns the
    /// existing title if one is already set).
    pub async fn session_generate_title(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "session.generate_title",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    pub async fn session_search(
        &self,
        query: &str,
        kiln_path: Option<&Path>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.search",
            SessionSearchRequest {
                query: query.to_string(),
                kiln: kiln_path.map(|p| p.to_string_lossy().to_string()),
                limit,
            },
        )
        .await
    }

    // =========================================================================
    // Session Observe RPC Methods
    // =========================================================================

    /// Load events from a persisted session's JSONL log.
    pub async fn session_load_events(&self, session_dir: &Path) -> Result<serde_json::Value> {
        self.typed_call(
            "session.load_events",
            SessionLoadEventsRequest {
                session_dir: session_dir.to_string_lossy().to_string(),
            },
        )
        .await
    }

    /// List persisted sessions from a kiln's session directory.
    pub async fn session_list_persisted(
        &self,
        kiln: &Path,
        session_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.list_persisted",
            SessionListPersistedRequest {
                kiln: kiln.to_string_lossy().to_string(),
                session_type: session_type.map(|t| t.to_string()),
                limit,
            },
        )
        .await
    }

    /// Render a persisted session's events to markdown.
    pub async fn session_render_markdown(
        &self,
        session_dir: &Path,
        include_timestamps: Option<bool>,
        include_tokens: Option<bool>,
        include_tools: Option<bool>,
        max_content_length: Option<usize>,
    ) -> Result<String> {
        let resp: SessionRenderMarkdownResponse = self
            .typed_call(
                "session.render_markdown",
                SessionRenderMarkdownRequest {
                    session_dir: session_dir.to_string_lossy().to_string(),
                    include_timestamps,
                    include_tokens,
                    include_tools,
                    max_content_length,
                },
            )
            .await?;
        Ok(resp.markdown)
    }

    /// Export a session to a markdown file.
    pub async fn session_export_to_file(
        &self,
        session_dir: &Path,
        output_path: Option<&Path>,
        include_timestamps: Option<bool>,
    ) -> Result<String> {
        let resp: SessionExportToFileResponse = self
            .typed_call(
                "session.export_to_file",
                SessionExportToFileRequest {
                    session_dir: session_dir.to_string_lossy().to_string(),
                    output_path: output_path.map(|p| p.to_string_lossy().to_string()),
                    include_timestamps,
                },
            )
            .await?;
        Ok(resp.output_path)
    }

    /// Clean up old persisted sessions.
    pub async fn session_cleanup(
        &self,
        kiln: &Path,
        older_than_days: u64,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.cleanup",
            SessionCleanupRequest {
                kiln: kiln.to_string_lossy().to_string(),
                older_than_days,
                dry_run,
            },
        )
        .await
    }

    /// Reindex persisted sessions into the kiln's NoteStore.
    pub async fn session_reindex(&self, kiln: &Path, force: bool) -> Result<serde_json::Value> {
        self.typed_call(
            "session.reindex",
            SessionReindexRequest {
                kiln: kiln.to_string_lossy().to_string(),
                force,
            },
        )
        .await
    }
}
