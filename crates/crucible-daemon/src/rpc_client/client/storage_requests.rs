//! Wire request types for the storage and kiln RPC methods.
//!
//! Split out of `storage.rs` because both sides of the wire now use them: the
//! client serializes each struct and the daemon's handler deserializes THE SAME
//! struct (gate A6), so these are the contract, not client-side plumbing.

/// Request for `kiln.open`.
///
/// `process` and `force` default because the server read them with
/// `optional_param!`: a caller that sends only `path` must keep working.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KilnOpenRequest {
    pub path: String,
    #[serde(default)]
    pub process: bool,
    #[serde(default)]
    pub force: bool,
}

/// Request for `kiln.set_classification`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KilnSetClassificationRequest {
    pub path: String,
    pub classification: String,
}

/// Request for `get_note_by_name`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GetNoteByNameRequest {
    pub kiln: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}

/// Request for `get_backlinks`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GetBacklinksRequest {
    pub kiln: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}

/// Request for `kiln.graph`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KilnGraphRequest {
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}

/// Request for `suggest_links`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuggestLinksRequest {
    pub kiln: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}

/// Request for `note.upsert`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteUpsertRequest {
    pub kiln: String,
    pub note: serde_json::Value,
}

/// Request for `note.list`. `scope` is the request authority; absent →
/// server defaults to `Scope::Workspace { path: kiln }`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteListRequest {
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}

/// Request for `note.get` and `note.delete`.
///
/// `note.get` accepts an optional `scope` field — the request authority.
/// When absent, the server defaults to `Scope::Workspace { path: kiln }`
/// (workspace-scoped read, which is the safest default for legacy callers
/// without a session context). `note.delete` ignores `scope`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NotePathRequest {
    pub kiln: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
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
///
/// Every field but `kiln_path` defaults, because the server used to read them
/// with `optional_param!` and substitute its own default. `transport` and
/// `port` stay `Option` so the substitution keeps happening in the handler,
/// where the default values are visible next to the call they configure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpStartRequest {
    pub kiln_path: String,
    #[serde(default)]
    pub no_just: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Accepted and ignored: it fed the annotation-scanned Lua tool discovery
    /// that `cru mcp` no longer does. Dropping it would break callers that
    /// still send it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub just_dir: Option<String>,
}

/// Request for `search_vectors`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent. Hits whose stored
/// `properties.scope` is outside the authority are filtered out at the SQL
/// layer, so out-of-scope notes never occupy result slots.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchVectorsRequest {
    pub kiln: String,
    pub vector: Vec<f32>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}

/// What an omitted `limit` means to `search_vectors`, kept where the field is
/// rather than in the handler's `unwrap_or`.
fn default_search_limit() -> usize {
    20
}

/// Request for `search_text`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchTextRequest {
    pub kiln: String,
    pub query: String,
    pub limit: usize,
}

/// One full-text hit: note path, title, a highlighted snippet, and the BM25
/// rank (lower is better).
#[derive(Debug, Clone)]
pub struct TextSearchHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub rank: f64,
}

/// Request for `embed.query`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbedQueryRequest {
    pub kiln: String,
    pub text: String,
}

/// Request for `search_grep` (ripgrep-style content search).
///
/// `root` must resolve inside a registered project or open kiln — the daemon
/// rejects anything else. `glob` filters by file name (e.g. `*.md`); `None`
/// searches all files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GrepSearchRequest {
    pub root: String,
    pub query: String,
    /// Compile `query` as a regex (Rust regex syntax) instead of matching it
    /// as a literal substring.
    #[serde(default)]
    pub regex: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    /// The handler still clamps this to `1..=GREP_MAX_LIMIT`.
    #[serde(default = "default_grep_limit")]
    pub limit: usize,
    /// Defaults to `true`, which is what the handler's `optional_param!` did.
    #[serde(default = "default_true")]
    pub case_insensitive: bool,
}

/// The `limit` an omitted field means — the handler's own constant, not a
/// copy of it, and the handler still clamps the value it gets.
fn default_grep_limit() -> usize {
    crate::server::grep::GREP_DEFAULT_LIMIT
}

fn default_true() -> bool {
    true
}

/// Request for `fs.list_dir`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsListDirRequest {
    pub root: String,
    pub rel_path: String,
    #[serde(default)]
    pub show_ignored: bool,
    #[serde(default)]
    pub show_hidden: bool,
}

/// Request for `fs.move`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsMoveRequest {
    pub root: String,
    /// `"project"` or `"kiln"` — selects the daemon-side allowlist.
    pub kind: String,
    pub from_rel: String,
    pub to_rel: String,
}

/// Request for `fs.mkdir` and `fs.trash`: one path inside one root.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FsPathRequest {
    pub root: String,
    /// `"project"` or `"kiln"` — selects the daemon-side allowlist.
    pub kind: String,
    pub rel_path: String,
}

/// Request for `note.rename` (and its `note.move` alias).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NoteRenameRequest {
    pub kiln: String,
    pub from_rel: String,
    pub to_rel: String,
}

/// Request for `scm.clone`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScmCloneRequest {
    pub url: String,
    /// Absolute, must not exist; overrides `[workspace] root_dir/<repo-name>`.
    #[serde(default)]
    pub dest: Option<String>,
    /// Overrides the repo name derived from the URL.
    #[serde(default)]
    pub name: Option<String>,
}

/// Request for `process_file`: index one file of one kiln.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessFileRequest {
    pub kiln: String,
    pub path: String,
}

/// Request for `list_notes`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListNotesRequest {
    pub kiln: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
}
