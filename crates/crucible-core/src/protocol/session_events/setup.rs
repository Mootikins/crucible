//! The seven session *setup* payloads, and the group enum over them.
//!
//! The daemon emits these during the setup task that runs immediately after
//! `session.create`. See the parent module for how a group enum becomes the
//! `{event, data}` pair on the wire.
//!
//! # Forward compatibility across a version skew
//!
//! `chat_runner/commands.rs` decodes these and, on failure, warns and drops the
//! event. So a payload an older `cru` cannot decode is a *feature it silently
//! loses*, not an error it reports. Unknown struct fields are ignored by serde
//! already, which leaves string-valued enums as the only sharp edge — one
//! unrecognised label fails the whole payload. [`ContextLimitSource`] is the
//! only one in this decode surface, and it carries a `#[serde(other)]`
//! fallback; the other payloads nest only structs of scalars (`ProviderInfo`,
//! `McpServerInfo`, `PluginStatusEntry`).
//!
//! [`crate::types::ToolSource`] was checked and needs nothing: it never crosses
//! this boundary as JSON. Tool provenance travels as a flat string built by
//! `format_tool_source` and read by `parse_tool_source`, which already returns
//! `None` for any spelling it does not know — the card then renders without a
//! badge instead of the event being dropped.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::mcp_status::McpServerInfo;
use crate::types::{PluginStatusEntry, ProviderInfo};

/// Setup-phase events, adjacently tagged so the enum's serialization *is* the
/// `{event, data}` pair the envelope carries.
///
/// `context_limit_resolved` is the one that is not exclusively a setup event.
/// A delegated agent has no endpoint or model for the daemon to query, so its
/// window arrives mid-turn instead (ACP `usage_update` →
/// [`TurnEvent::ContextWindow`](crate::turn::TurnEvent::ContextWindow)) and the
/// daemon re-emits the same event from the turn stream. Consumers treat it as a
/// plain assignment, so a late one needs no special case.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum SetupPayload {
    SessionInitialized(SessionInitializedPayload),
    ProvidersListed(ProvidersListedPayload),
    ContextLimitResolved(ContextLimitResolvedPayload),
    WorkspaceIndexed(WorkspaceIndexedPayload),
    KilnNotesIndexed(KilnNotesIndexedPayload),
    PluginsDiscovered(PluginsDiscoveredPayload),
    McpServersReady(McpServersReadyPayload),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInitializedPayload {
    pub model: String,
    pub mode: String,
    pub agent_name: Option<String>,
    pub kiln_path: PathBuf,
    pub workspace_path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProvidersListedPayload {
    pub providers: Vec<ProviderInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextLimitResolvedPayload {
    pub limit: usize,
    pub source: ContextLimitSource,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextLimitSource {
    ProviderApi,
    Config,
    Default,
    /// The agent reported its own window. Only a delegated (ACP) agent can:
    /// it has no endpoint or model for the daemon to query, so the number
    /// arrives on the wire in a `usage_update` frame instead.
    Agent,

    /// A source this build does not know — a newer daemon than this `cru`.
    ///
    /// Without this the unknown string fails the whole
    /// [`ContextLimitResolvedPayload`] decode, so `chat_runner/commands.rs`
    /// warns and drops the event and the statusline keeps its "no data" path:
    /// an older client loses a `limit` it could have rendered perfectly well,
    /// because it did not recognise the label saying where the number came
    /// from. The number is what the user sees; the provenance is not rendered
    /// anywhere.
    ///
    /// Never constructed by this crate — only produced by deserialization.
    /// The mirror of the care taken on `TurnEvent::ToolCall::diffs`
    /// (`#[serde(default, skip_serializing_if)]`), which is the same
    /// old-client-new-daemon problem on the other kind of field.
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceIndexedPayload {
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KilnNotesIndexedPayload {
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginsDiscoveredPayload {
    pub plugins: Vec<PluginStatusEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServersReadyPayload {
    pub servers: Vec<McpServerInfo>,
}
