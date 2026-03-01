//! Daemon client implementation
//!
//! Provides a client for communicating with the Crucible daemon over Unix sockets.
//! Supports both request/response RPC calls and asynchronous event streaming.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, trace, warn};

/// Session event received from daemon
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Daemon capabilities returned by `daemon.capabilities` RPC
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DaemonCapabilities {
    pub version: String,
    #[serde(default)]
    pub build_sha: Option<String>,
    pub protocol_version: String,
    pub capabilities: CapabilityFlags,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapabilityFlags {
    pub kilns: bool,
    pub sessions: bool,
    pub agents: bool,
    pub events: bool,
    pub thinking_budget: bool,
    pub model_switching: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionRequest {
    pub session_id: String,
    pub kiln_path: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionResponse {
    pub session_id: String,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
    #[serde(default)]
    pub views: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterHooksRequest {
    pub session_id: String,
    pub hooks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterHooksResponse {
    pub status: String,
    pub registered: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaExecuteHookRequest {
    pub session_id: String,
    pub hook_name: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaExecuteHookResponse {
    pub executed: usize,
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaShutdownSessionRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaShutdownSessionResponse {
    pub shutdown: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaDiscoverPluginsRequest {
    pub kiln_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaDiscoverPluginsResponse {
    #[serde(default)]
    pub plugins: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaPluginHealthRequest {
    pub plugin_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaPluginHealthResponse {
    pub name: String,
    pub healthy: bool,
    #[serde(default)]
    pub checks: Vec<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaGenerateStubsRequest {
    pub output_dir: String,
    #[serde(default)]
    pub verify: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaGenerateStubsResponse {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsRequest {
    pub test_path: String,
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsResponse {
    pub passed: usize,
    pub failed: usize,
    pub load_failures: usize,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterCommandsRequest {
    pub session_id: String,
    pub commands: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterCommandsResponse {
    pub registered: usize,
}

// =========================================================================
// Generic RPC Request Types
// =========================================================================

/// Empty request for methods that take no parameters.
#[derive(Debug, Clone, serde::Serialize)]
struct EmptyParams {}

/// Request for methods that take only a kiln path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KilnPathRequest {
    pub kiln: String,
}

/// Request for methods that take only a filesystem path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathRequest {
    pub path: String,
}

/// Request for methods that take only a name.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NameRequest {
    pub name: String,
}

/// Request for `kiln.open`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KilnOpenRequest {
    pub path: String,
    pub process: bool,
    pub force: bool,
}

/// Request for `kiln.set_classification`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KilnSetClassificationRequest {
    pub path: String,
    pub classification: String,
}

/// Request for `get_note_by_name`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GetNoteByNameRequest {
    pub kiln: String,
    pub name: String,
}

/// Request for `note.upsert`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteUpsertRequest {
    pub kiln: String,
    pub note: serde_json::Value,
}

/// Request for `note.get` and `note.delete`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NotePathRequest {
    pub kiln: String,
    pub path: String,
}

/// Request for `process_batch`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessBatchRequest {
    pub kiln: String,
    pub paths: Vec<String>,
}

/// Request for `storage.backup`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageBackupRequest {
    pub kiln: String,
    pub dest: String,
}

/// Request for `storage.restore`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageRestoreRequest {
    pub kiln: String,
    pub source: String,
}

/// Request for `mcp.start`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct McpStartRequest {
    pub kiln_path: String,
    pub no_just: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub just_dir: Option<String>,
}

/// Request for `skills.list`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillsListRequest {
    pub kiln_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_filter: Option<String>,
}

/// Request for `skills.get`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillsGetRequest {
    pub name: String,
    pub kiln_path: String,
}

/// Request for `skills.search`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillsSearchRequest {
    pub query: String,
    pub kiln_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Return value for `subscribe_process_events`.
#[derive(Debug, Clone, serde::Serialize)]
struct ProcessEventsSubscription {
    batch_id: String,
    subscription: serde_json::Value,
}

// =========================================================================
// Session RPC Request/Response Types (Phase 1)
// =========================================================================

/// Request for `session.create`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionCreateRequest {
    #[serde(rename = "type")]
    pub session_type: String,
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_kilns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_path: Option<String>,
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

/// Shared request for `session.subscribe` and `session.unsubscribe`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSubscribeRequest {
    pub session_ids: Vec<String>,
}

/// Request for `session.configure_agent`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionConfigureAgentRequest {
    pub session_id: String,
    pub agent: serde_json::Value,
}

/// Request for `session.send_message`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSendMessageRequest {
    pub session_id: String,
    pub content: String,
}

/// Request for `session.interaction_respond`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInteractionRespondRequest {
    pub session_id: String,
    pub request_id: String,
    pub response: serde_json::Value,
}

/// Request for `session.switch_model`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSwitchModelRequest {
    pub session_id: String,
    pub model_id: String,
}

/// Request for `session.set_title`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetTitleRequest {
    pub session_id: String,
    pub title: String,
}

/// Request for `session.set_thinking_budget`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetThinkingBudgetRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,
}

/// Request for `session.set_precognition`.
///
/// NOTE: The client sends `precognition_enabled` but the daemon handler reads `enabled`.
/// This is a pre-existing field name mismatch.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetPrecognitionRequest {
    pub session_id: String,
    pub precognition_enabled: bool,
}

/// Request for `session.set_temperature`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetTemperatureRequest {
    pub session_id: String,
    pub temperature: f64,
}

/// Request for `session.set_max_tokens`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetMaxTokensRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
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

/// Request for `models.list` (no active session required).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListAllModelsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiln_path: Option<String>,
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

/// Request for `search_vectors`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchVectorsRequest {
    pub kiln: String,
    pub vector: Vec<f32>,
    pub limit: usize,
}

/// Request for `list_notes`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListNotesRequest {
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_filter: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    Match,
    Mismatch { client: String, daemon: String },
}

impl VersionCheck {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }
}

type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>;

/// Extract a string array from a JSON value at the given key.
fn extract_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Client for communicating with the Crucible daemon
///
/// The client supports two modes:
/// - Simple mode: Created with `connect()` or `connect_to()`, suitable for RPC-only usage
/// - Event mode: Created with `connect_with_events()`, supports both RPC and async events
///
/// In event mode, a background task continuously reads from the socket, routing:
/// - RPC responses to their waiting callers
/// - Async events to the event channel
pub struct DaemonClient {
    writer: Arc<Mutex<OwnedWriteHalf>>,
    next_id: AtomicU64,
    pending_requests: PendingRequests,
    reader_task: Option<JoinHandle<()>>,
    // For simple mode (no background reader)
    simple_reader: Option<Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>>,
}

impl Drop for DaemonClient {
    fn drop(&mut self) {
        if let Some(task) = self.reader_task.take() {
            task.abort();
        }
    }
}
// Re-export types from parent module
