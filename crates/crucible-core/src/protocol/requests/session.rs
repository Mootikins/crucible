//! Wire types of the `session` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

use crate::config::KilnName;
use std::path::PathBuf;
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
    /// spelling the Lua binding always used (`cru.session.create{ kilns =
    /// {...} }`), so plugins keep working; a caller still sending `kiln` or
    /// `connect_kilns` now gets the default set, which is the intended break.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kilns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// A workspace that a plugin provides, for example `worktree:feat/x`. The
    /// daemon resolves it before the create and writes the path to
    /// `workspace`. A target that no plugin resolves refuses the create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_target: Option<String>,
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
    pub tool_policy: Option<crate::agent::ToolPolicyMap>,
    /// Internal-agent overrides applied on top of config-derived defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Explicit ACP environment values, merged over the selected profile.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env_overrides: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// `Some([])` disables configured MCP servers for this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<String>>,
    /// The plugin that creates the session. The Lua binding of
    /// `cru.session.create` writes it from the running plugin and removes a
    /// value that the Lua caller supplies. The daemon stores it on every
    /// type. A proposal names this plugin as its author only on a `plugin`
    /// session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
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
    pub env_overrides: std::collections::HashMap<String, String>,
    pub system_prompt: Option<String>,
    pub mcp_servers: Option<Vec<String>>,
}

/// Request for `session.list`.
///
/// The daemon ignores a `type` or a `state` that it does not know. It does
/// not refuse the listing.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionListRequest {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    /// The caller's whole kiln set. A session is listed when its own set
    /// overlaps it. An empty set lists what the daemon can see.
    #[serde(
        default,
        alias = "kiln",
        deserialize_with = "super::common::kiln_set",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub kilns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_archived: Option<bool>,
    /// Include delegated child sessions (hidden by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_children: Option<bool>,
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

/// Request for `session.events_after`.
///
/// `after` is the caller's seq cursor: the last event it APPLIED. The reply
/// carries the persisted wire envelopes strictly past it, in order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionEventsAfterRequest {
    pub session_id: String,
    pub after: u64,
}

/// Request for `session.send_message`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSendMessageRequest {
    pub session_id: String,
    pub content: String,
    /// An absent value means an interactive turn.
    #[serde(default = "super::common::default_true")]
    pub is_interactive: bool,
    /// The daemon ignores a mode that it does not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// The stored review comments that the message attaches. The daemon
    /// resolves each one into a context block. `null` reads as no comments.
    #[serde(
        default,
        deserialize_with = "super::common::null_as_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub comments: Vec<crate::diff::CommentRef>,
}

/// Request for `session.interaction_respond`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionInteractionRespondRequest {
    pub session_id: String,
    pub request_id: String,
    pub response: serde_json::Value,
}

/// Request for `session.inject_context`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionInjectContextRequest {
    pub session_id: String,
    /// `system`, `user` or `assistant` — anything else is `INVALID_PARAMS`.
    pub role: String,
    pub content: String,
}

/// Request for `session.test_interaction` — the developer-facing prod that
/// emits an `interaction_requested` event nobody is waiting on.
///
/// Every field but `session_id` is optional and every default is a canned
/// example, because the method exists to check that a client renders a modal
/// at all.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionTestInteractionRequest {
    pub session_id: String,
    /// `ask` (the default) or `permission`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub interaction_type: Option<String>,
    /// The question an `ask` puts to the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// The command a `permission` asks about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Request for `session.fork`.
///
/// `session_id` names the PARENT; the fork reports its own id as `id`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionForkRequest {
    pub session_id: String,
    /// Copy only the first N user/assistant/system messages. All of them when
    /// omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up_to: Option<u64>,
}

/// Request for `session.dismiss_notification`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionDismissNotificationRequest {
    pub session_id: String,
    pub notification_id: String,
}

/// Request for `session.set_title`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSetTitleRequest {
    pub session_id: String,
    pub title: String,
}

/// Request for `session.search`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSearchRequest {
    pub query: String,
    /// The caller's whole kiln set — results are the sessions overlapping it.
    /// Always sent, empty included: an empty scope overlaps nothing, which is
    /// the fail-closed answer a kiln-less session should get.
    #[serde(default, alias = "kiln", deserialize_with = "super::common::kiln_set")]
    pub kilns: Vec<String>,
    /// An absent value returns at most 20 matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Request for `session.list_persisted`.
///
/// `kilns` is the caller's whole kiln set, not directories to scan: the daemon
/// returns the sessions whose own set overlaps it — the same predicate
/// `session.search` and `session.cleanup` answer to.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionListPersistedRequest {
    #[serde(default, alias = "kiln", deserialize_with = "super::common::kiln_set")]
    pub kilns: Vec<String>,
    /// The daemon ignores a type that it does not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    /// An absent value returns at most 50 sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionCleanupRequest {
    #[serde(default, alias = "kiln", deserialize_with = "super::common::kiln_set")]
    pub kilns: Vec<String>,
    pub older_than_days: u64,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub all_kilns: bool,
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

/// Reply from `session.list`.
///
/// One `SessionSummary` per session, listing-shaped: see
/// `crate::session::SessionSummary` for which fields a listing fills.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionListReply {
    pub sessions: Vec<crate::session::SessionSummary>,
    /// How many sessions the reply carries.
    pub total: usize,
}
