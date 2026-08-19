//! Session RPC methods
//!
//! Methods for managing chat sessions, sending messages, and configuring agents.

use anyhow::Result;
use crucible_core::config::KilnName;
use std::path::{Path, PathBuf};

use super::DaemonClient;

// =========================================================================
// Session RPC Request/Response Types
// =========================================================================

/// Request for `session.create`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionCreateRequest {
    /// Defaulted rather than required because the server now *deserializes*
    /// this struct instead of hand-plucking `params["type"]` with an
    /// `.unwrap_or("chat")`. Without the serde default, omitting `type` — which
    /// several callers do — would start failing as `INVALID_PARAMS`.
    #[serde(rename = "type", default = "default_session_type")]
    pub session_type: String,
    /// The session's whole kiln set — flat, no member privileged. Omitted or
    /// empty → the daemon resolves its default (home kiln); keeping that
    /// fallback daemon-side means clients can never drift from it.
    ///
    /// Replaces the pre-flatten `kiln` + `connect_kilns` pair. `kilns` is the
    /// spelling the Lua binding always used (`cru.sessions.create{ kilns =
    /// {...} }`), so plugins keep working; a caller still sending `kiln` or
    /// `connect_kilns` now gets the default set, which is the intended break.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kilns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_path: Option<String>,
    /// "acp" | "internal"; None treated as "internal" for back-compat.
    /// Lets the daemon's setup task branch on agent type at create time,
    /// before `session.configure_agent` has been called.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,

    /// Isolation override, forwarded untouched to the plugin that resolves it:
    /// `false` (no container even if the project has one), `true` (the default
    /// profile), a profile name, or an environment object. Untyped on purpose
    /// — the vocabulary belongs to the isolating plugin, not to this client.
    /// Absent must stay absent: it means "resolve normally", which is a
    /// different instruction from `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<serde_json::Value>,

    /// When true, the daemon resolves and configures the session's agent as
    /// part of create (ACP profile for `agent_type == "acp"`, otherwise
    /// config-derived internal defaults), and returns the resolved model in
    /// `agent_model`. Absent/false ⇒ today's behavior: the session is created
    /// agent-less and the caller configures it separately via
    /// `session.configure_agent`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub configure_agent: bool,
    /// ACP profile name; used when `configure_agent` and `agent_type == "acp"`.
    ///
    /// DEPRECATED on an internal session, where it is an alias for
    /// [`Self::agent_card`]. It still resolves an agent card there because
    /// `crucible-web` sends exactly that shape, but new callers should say
    /// `agent_card`: one field cannot mean both "launch this ACP subprocess"
    /// and "use this internal agent card" without `agent_type` silently
    /// deciding which. Setting both fields is `INVALID_PARAMS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Agent-card name for an internal session (a specialized internal agent:
    /// card prompt/model/tools over the config-derived defaults). Ignored when
    /// `agent_type == "acp"`, which selects a profile via `agent_name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_card: Option<String>,
    /// Per-tool `allow`/`ask`/`deny` for this session's agent, applied last —
    /// after an agent card's own `tools:` block.
    ///
    /// Exists because it is the one part of an agent that a caller cannot
    /// express at create and therefore has to walk back afterwards with
    /// `session.configure_agent`, which is a whole-agent *replacement*: a
    /// caller that resolved a card at create and then re-configured to set a
    /// tool policy would silently discard the card's prompt and model. The
    /// Discord plugin does exactly that, per Discord sender.
    ///
    /// No new authority: `session.configure_agent` already lets any caller on
    /// this socket set any tool policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
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

/// The session type an omitted `type` means. Mirrors what the server's
/// hand-plucking used to do (`optional_param!(req, "type", …).unwrap_or("chat")`).
fn default_session_type() -> String {
    "chat".to_string()
}

/// Parameters for creating a session.
#[derive(Debug, Clone)]
pub struct SessionCreateParams {
    pub session_type: String,
    /// The session's whole kiln set, by registry NAME. Empty is a legitimate
    /// value, not a request for a default: it creates a tools-only session with
    /// no corpus (§4.1). The daemon no longer substitutes its data root, which
    /// is the parent of the sessions root and would put every transcript in
    /// scope.
    ///
    /// Names rather than paths because the daemon resolves them against the
    /// `[kilns]` registry: a path here would name a directory the registration
    /// floor never saw, which is the door names exist to close.
    pub kilns: Vec<KilnName>,
    pub workspace: Option<PathBuf>,
    pub recording_mode: Option<String>,
    pub recording_path: Option<PathBuf>,
    /// "acp" | "internal"; None treated as "internal" for back-compat.
    pub agent_type: Option<String>,
    /// Isolation override; see [`SessionCreateRequest::isolation`]. `None`
    /// (the overwhelmingly common case) omits the field entirely.
    pub isolation: Option<serde_json::Value>,
}

/// Optional agent spec for `session.create` that asks the daemon to resolve and
/// configure the session's agent server-side (the "daemon owns defaults" path).
///
/// `agent_name` selects an ACP profile (with `agent_type == "acp"`);
/// `agent_card` selects an agent card on an internal session; the
/// provider/model/endpoint fields override internal-agent config defaults. An
/// all-`None` spec on an internal session means "use the config defaults as-is".
#[derive(Debug, Clone, Default)]
pub struct SessionAgentSpec {
    pub agent_name: Option<String>,
    /// Agent-card name for an internal session. Mutually exclusive with
    /// `agent_name` — the daemon refuses both (`INVALID_PARAMS`).
    pub agent_card: Option<String>,
    pub provider: Option<String>,
    pub provider_key: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
}

/// Build the wire request. `agent = Some(..)` sets `configure_agent = true` so
/// the daemon resolves + configures the agent as part of create; `None` keeps
/// the back-compat "create agent-less, configure later" shape.
pub(super) fn build_create_request(
    params: SessionCreateParams,
    agent: Option<SessionAgentSpec>,
) -> SessionCreateRequest {
    let configure_agent = agent.is_some();
    let agent = agent.unwrap_or_default();
    SessionCreateRequest {
        session_type: params.session_type,
        kilns: if params.kilns.is_empty() {
            None
        } else {
            Some(params.kilns.iter().map(KilnName::to_string).collect())
        },
        workspace: params.workspace.map(|ws| ws.to_string_lossy().to_string()),
        recording_mode: params.recording_mode,
        recording_path: params
            .recording_path
            .map(|p| p.to_string_lossy().to_string()),
        agent_type: params.agent_type,
        isolation: params.isolation,
        configure_agent,
        agent_name: agent.agent_name,
        agent_card: agent.agent_card,
        // No Rust client sets a per-session tool policy at create; the plugin
        // bridge deserializes the request straight from a Lua table.
        tool_policy: None,
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
/// Used by: `session.get`, `session.status`, `session.pause`, `session.resume`,
/// `session.end`, `session.cancel`, `session.list_models`, `session.list_modes`,
/// `session.list_notifications`, `session.load_events`,
/// `session.get_thinking_budget`, `session.get_precognition`,
/// `session.get_temperature`, `session.get_max_tokens`, `session.archive`,
/// `session.unarchive`, `session.delete`, `review.list_hunks`, `review.rebase`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionIdRequest {
    pub session_id: String,
}

/// Request for `session.replay`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionReplayRequest {
    pub recording_path: String,
    /// Real time when omitted, which is what the handler's `unwrap_or` did.
    #[serde(default = "default_replay_speed")]
    pub speed: f64,
}

fn default_replay_speed() -> f64 {
    1.0
}

/// Request for `session.resume_from_storage`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionResumeFromStorageRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSetTitleRequest {
    pub session_id: String,
    pub title: String,
}

/// Request for `session.search`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSearchRequest {
    pub query: String,
    /// The caller's whole kiln set — results are the sessions overlapping it.
    /// Always sent, empty included: an empty scope overlaps nothing, which is
    /// the fail-closed answer a kiln-less session should get.
    pub kilns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Request for `session.list_persisted`.
///
/// `kilns` is the caller's whole kiln set, not directories to scan: the daemon
/// returns the sessions whose own set overlaps it — the same predicate
/// `session.search` and `session.cleanup` answer to.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionListPersistedRequest {
    pub kilns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Request for `session.render_markdown`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionRenderMarkdownRequest {
    pub session_id: String,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionExportToFileRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timestamps: Option<bool>,
}

/// Request for `session.cleanup`.
///
/// `kilns` is the caller's whole kiln set; deletion is scoped to the sessions
/// overlapping it. `all_kilns` widens that to every session on the machine and
/// has to be set deliberately — sessions live in one flat root now, so an
/// unscoped sweep is not recoverable.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionCleanupRequest {
    pub kilns: Vec<String>,
    pub older_than_days: u64,
    pub dry_run: bool,
    pub all_kilns: bool,
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
        kiln: Option<&KilnName>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
        include_archived: Option<bool>,
    ) -> Result<serde_json::Value> {
        self.session_list_with_children(
            kiln,
            workspace,
            session_type,
            state,
            include_archived,
            None,
        )
        .await
    }

    /// `session.list` with explicit control over delegated-child visibility
    /// (children are hidden unless `include_children` is `Some(true)`).
    pub async fn session_list_with_children(
        &self,
        kiln: Option<&KilnName>,
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
                kiln: kiln.map(KilnName::to_string),
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

    /// `session.status` — the status slots plugins published for a session.
    ///
    /// Returned as raw JSON (`{"status": [{key, plugin, text, level}, …]}`):
    /// the slots are keyed so any client renders any plugin's state without
    /// knowing which plugins exist, and typing them here would be the first
    /// step toward this client interpreting them.
    pub async fn session_status(&self, session_id: &str) -> Result<serde_json::Value> {
        self.session_id_call("session.status", session_id).await
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

    pub async fn session_delete(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "session.delete",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    pub async fn session_archive(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "session.archive",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    pub async fn session_unarchive(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "session.unarchive",
            SessionIdRequest {
                session_id: session_id.to_string(),
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
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.resume_from_storage",
            SessionResumeFromStorageRequest {
                session_id: session_id.to_string(),
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

    /// Search session transcripts within `kilns` — the caller's whole kiln set,
    /// not one member of it. Scope is kiln-set *overlap*, so a caller that
    /// sends a subset silently hides the sessions sharing the rest.
    pub async fn session_search(
        &self,
        query: &str,
        kilns: &[KilnName],
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.search",
            SessionSearchRequest {
                query: query.to_string(),
                kilns: kilns.iter().map(KilnName::to_string).collect(),
                limit,
            },
        )
        .await
    }

    // =========================================================================
    // Session Observe RPC Methods
    // =========================================================================

    /// Load events from a persisted session's JSONL log.
    pub async fn session_load_events(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "session.load_events",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    /// List persisted sessions within `kilns` — the caller's whole kiln set,
    /// not one member of it. Scope is kiln-set *overlap*, so a caller that
    /// sends a subset silently hides the sessions sharing the rest.
    pub async fn session_list_persisted(
        &self,
        kilns: &[KilnName],
        session_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.list_persisted",
            SessionListPersistedRequest {
                kilns: kilns.iter().map(KilnName::to_string).collect(),
                session_type: session_type.map(|t| t.to_string()),
                limit,
            },
        )
        .await
    }

    /// Render a persisted session's events to markdown.
    pub async fn session_render_markdown(
        &self,
        session_id: &str,
        include_timestamps: Option<bool>,
        include_tokens: Option<bool>,
        include_tools: Option<bool>,
        max_content_length: Option<usize>,
    ) -> Result<String> {
        let resp: SessionRenderMarkdownResponse = self
            .typed_call(
                "session.render_markdown",
                SessionRenderMarkdownRequest {
                    session_id: session_id.to_string(),
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
        session_id: &str,
        output_path: Option<&Path>,
        include_timestamps: Option<bool>,
    ) -> Result<String> {
        let resp: SessionExportToFileResponse = self
            .typed_call(
                "session.export_to_file",
                SessionExportToFileRequest {
                    session_id: session_id.to_string(),
                    output_path: output_path.map(|p| p.to_string_lossy().to_string()),
                    include_timestamps,
                },
            )
            .await?;
        Ok(resp.output_path)
    }

    /// Clean up old persisted sessions.
    ///
    /// `kilns` is the caller's whole kiln set; `all_kilns` sweeps every session
    /// on the machine and is refused unless set. One of the two has to say
    /// something — an empty `kilns` with `all_kilns: false` is an error, not a
    /// silent no-op, because this verb deletes.
    pub async fn session_cleanup(
        &self,
        kilns: &[KilnName],
        older_than_days: u64,
        dry_run: bool,
        all_kilns: bool,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.cleanup",
            SessionCleanupRequest {
                kilns: kilns.iter().map(KilnName::to_string).collect(),
                older_than_days,
                dry_run,
                all_kilns,
            },
        )
        .await
    }
}
