//! Wire types of the `storage` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

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

/// Request for `kiln.register`.
///
/// `auto` and `make_default` default because they answer questions the plain
/// `cru kiln register <name> <path>` does not ask: `auto` records that Crucible
/// derived the entry rather than the user naming it, and `make_default` is the
/// chat preflight's answer to "which kiln does every future command use".
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KilnRegisterRequest {
    /// The name to bind, or `None` to let the daemon derive one.
    ///
    /// Optional because the derivation depends on what is already registered
    /// (`notes`, then `notes-2`), and only the daemon's registry knows that.
    /// A caller that derived its own name would be deriving against a
    /// different set.
    #[serde(default)]
    pub name: Option<String>,
    pub path: String,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub make_default: bool,
}

/// Request for `llm.register_provider`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmRegisterProviderRequest {
    /// The provider key, which is also its `BackendType` name.
    pub provider: String,
    /// The model to use by default with it.
    pub model: String,
    #[serde(default)]
    pub make_default: bool,
}

/// Request for `get_note_by_name`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetNoteByNameRequest {
    pub kiln: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `get_backlinks`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetBacklinksRequest {
    pub kiln: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `kiln.graph`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KilnGraphRequest {
    pub kiln: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `suggest_links`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SuggestLinksRequest {
    pub kiln: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `note.upsert`.
///
/// `note` stays a `Value`: the handler answers a distinct
/// `Invalid note record: {e}` for a `note` that is not a `NoteRecord`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NoteUpsertRequest {
    pub kiln: String,
    pub note: serde_json::Value,
}

/// Request for `note.list`. `scope` is the request authority; absent →
/// server defaults to `Scope::Workspace { path: kiln }`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NoteListRequest {
    pub kiln: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `note.get` and `note.delete`.
///
/// `note.get` accepts an optional `scope` field — the request authority.
/// When absent, the server defaults to `Scope::Workspace { path: kiln }`
/// (workspace-scoped read, which is the safest default for legacy callers
/// without a session context). `note.delete` ignores `scope`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotePathRequest {
    pub kiln: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `process_batch`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub scope: Option<crate::storage::Scope>,
}

/// What an omitted `limit` means to `search_vectors`, kept where the field is
/// rather than in the handler's `unwrap_or`.
fn default_search_limit() -> usize {
    20
}

/// One row of a `search_vectors` reply.
///
/// The daemon answers from the block-first search, the same path the search
/// tool and precognition read. `block` says which passage answered; it is
/// absent when the kiln has note vectors only. `snippet` is the block's own
/// text, so a client can quote the passage without a second round trip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorHit {
    pub document_id: String,
    pub score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<crate::types::database::BlockRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Reduce a block-ranked reply to one row per note, in reply order.
///
/// Several blocks of one note arrive as several rows. A caller that ranks
/// notes (the eval, the note store) keeps the first row of each note, so a
/// note with many blocks near the query occupies one rank, not several.
pub fn first_per_note(hits: Vec<VectorHit>) -> Vec<VectorHit> {
    let mut seen = std::collections::HashSet::new();
    hits.into_iter()
        .filter(|hit| seen.insert(hit.document_id.clone()))
        .collect()
}

#[cfg(test)]
mod first_per_note_tests {
    use super::*;

    fn hit(document_id: &str, span_start: usize) -> VectorHit {
        VectorHit {
            document_id: document_id.to_string(),
            score: 1.0 - span_start as f64 / 100.0,
            block: Some(crate::types::database::BlockRef {
                span_start,
                span_end: span_start + 10,
                kind: "paragraph".to_string(),
                cited: Vec::new(),
            }),
            snippet: None,
        }
    }

    #[test]
    fn keeps_the_first_row_of_each_note_in_reply_order() {
        let rows = vec![
            hit("a.md", 0),
            hit("a.md", 10),
            hit("b.md", 20),
            hit("a.md", 30),
        ];
        let notes = first_per_note(rows);
        let ids: Vec<&str> = notes.iter().map(|h| h.document_id.as_str()).collect();
        assert_eq!(ids, vec!["a.md", "b.md"]);
        assert_eq!(notes[0].block.as_ref().unwrap().span_start, 0);
    }
}

/// Request for `search_text`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchTextRequest {
    pub kiln: String,
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

/// Request for `embed.query`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[serde(default = "super::common::default_true")]
    pub case_insensitive: bool,
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

/// Request for `diff.get`.
///
/// For a branch source, an empty `base` asks the daemon for the default
/// branch. The reply then names the branch that the daemon used.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffGetRequest {
    pub source: crate::diff::DiffsetSource,
}

/// Request for `diff.file`: one file of the diffset of `source`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffFileRequest {
    pub source: crate::diff::DiffsetSource,
    /// The path relative to the root, on the current side.
    pub path: String,
    /// The old path of a renamed file. The base text comes from this path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The root of the file. A session record needs it, because a session
    /// can have more than one root. A branch source names its own root, so
    /// a branch request omits it or repeats the root of the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<crate::session::PhysicalRoot>,
}

/// Request for `diff.comment`: anchor a comment to a line range of one file
/// of the diffset of `source`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffCommentRequest {
    pub source: crate::diff::DiffsetSource,
    /// The root of the file. A session record needs it, as in
    /// [`DiffFileRequest::root`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<crate::session::PhysicalRoot>,
    /// The path relative to the root, on the current side.
    pub path: String,
    /// The old path of a renamed file. A comment on the base side quotes
    /// the text of this path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The side that the line numbers count on.
    pub side: crate::session::CommentSide,
    /// The first line, 1-based.
    pub line_start: u32,
    /// One past the last line. Absent means `line_start + 1`: one line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    pub body: String,
    /// Absent means a human wrote the comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<crate::session::CommentAuthor>,
}

/// What `diff.comment` answers: the stored comment and its diffset.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffCommentReply {
    pub diffset: crate::diff::DiffsetId,
    pub comment: crate::session::Comment,
}

/// Request for `diff.resolve_comment`: mark one comment of the diffset of
/// `source` resolved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffResolveCommentRequest {
    pub source: crate::diff::DiffsetSource,
    pub comment_id: String,
}

/// What `diff.resolve_comment` answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffResolveCommentReply {
    pub diffset: crate::diff::DiffsetId,
    pub comment_id: String,
    pub resolved: bool,
}

/// Request for `diff.delete_comment`: remove one comment of the diffset of
/// `source` from the store.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffDeleteCommentRequest {
    pub source: crate::diff::DiffsetSource,
    pub comment_id: String,
}

/// What `diff.delete_comment` answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffDeleteCommentReply {
    pub diffset: crate::diff::DiffsetId,
    pub comment_id: String,
    pub deleted: bool,
}

/// Request for `diff.comments`: the comments of the diffset of `source`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffCommentsRequest {
    pub source: crate::diff::DiffsetSource,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListNotesRequest {
    pub kiln: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// One row of `list_notes`, as it crosses the RPC wire.
///
/// A struct rather than a tuple: it grew a sixth field, and a six-tuple at
/// three call sites is a puzzle rather than a type.
#[derive(Debug, Clone, Default)]
pub struct NoteListRow {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: Option<String>,
    pub properties: std::collections::BTreeMap<String, serde_json::Value>,
}

impl NoteListRow {
    /// The legacy five-field view, for callers that want no properties.
    pub fn into_parts(self) -> (String, String, Option<String>, Vec<String>, Option<String>) {
        (self.name, self.path, self.title, self.tags, self.updated_at)
    }
}

/// What `diff.comments` answers: each comment, oldest first, with its range
/// projected onto the current text of its side.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffCommentsReply {
    pub diffset: crate::diff::DiffsetId,
    pub comments: Vec<ListedComment>,
}

/// A comment as the daemon lists it: its range follows its text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ListedComment {
    /// The stored comment. When its text moved, `line_range` is the new range.
    pub comment: crate::session::Comment,
    /// The current text of the side does not contain the quoted text.
    /// The pane shows an outdated comment at the end of its file.
    pub outdated: bool,
}

/// The hit cap of `search_grep` when the caller omits `limit`.
pub const GREP_DEFAULT_LIMIT: usize = 100;

/// The `limit` an omitted field means. The handler still clamps the value.
fn default_grep_limit() -> usize {
    GREP_DEFAULT_LIMIT
}
