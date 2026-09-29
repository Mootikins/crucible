//! Wire types of the `session` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

use crate::config::KilnName;
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

impl SessionCreateRequest {
    /// The wire form of a kiln set, by registry NAME.
    ///
    /// An empty set is absent on the wire, so the daemon resolves its
    /// default set. Names, not paths: the daemon resolves them against the
    /// `[kilns]` registry, and a path would name a directory that the
    /// registration floor never saw.
    pub fn kiln_set(kilns: impl IntoIterator<Item = KilnName>) -> Option<Vec<String>> {
        let kilns: Vec<String> = kilns.into_iter().map(|kiln| kiln.to_string()).collect();
        (!kilns.is_empty()).then_some(kilns)
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The session type an omitted `type` means. Mirrors what the server's
/// hand-plucking used to do (`optional_param!(req, "type", …).unwrap_or("chat")`).
fn default_session_type() -> String {
    "chat".to_string()
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

/// The body of `session.events_after`, inside `Scoped`.
///
/// `after` is the caller's seq cursor: the last event it APPLIED. The reply
/// carries the persisted wire envelopes strictly past it, in order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventCursor {
    pub after: u64,
}

/// The body of `session.send_message`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInput {
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

/// The body of `session.interaction_respond`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InteractionAnswer {
    pub request_id: String,
    pub response: serde_json::Value,
}

/// The body of `session.inject_context`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextInjection {
    /// `system`, `user` or `assistant` — anything else is `INVALID_PARAMS`.
    pub role: String,
    pub content: String,
}

/// The body of `session.test_interaction`, inside `Scoped`.
///
/// The method is the developer-facing prod that emits an
/// `interaction_requested` event nobody is waiting on.
///
/// Every field is optional and every default is a canned
/// example, because the method exists to check that a client renders a modal
/// at all.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestInteraction {
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

/// The body of `session.fork`, inside `Scoped`.
///
/// The session id of the envelope names the PARENT. The fork reports its
/// own id as `id`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForkPoint {
    /// Copy only the first N user/assistant/system messages. All of them when
    /// omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up_to: Option<u64>,
}

/// The body of `session.dismiss_notification`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationKey {
    pub notification_id: String,
}

/// The body of `session.set_title`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Title {
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

/// The body of `session.render_markdown`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkdownOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timestamps: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tokens: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_length: Option<usize>,
}

/// The body of `session.export_to_file`, inside `Scoped`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportOptions {
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
