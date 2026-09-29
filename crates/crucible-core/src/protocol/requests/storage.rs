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

/// One note of one kiln, found by its name: the request of
/// `get_note_by_name` and `get_backlinks`.
///
/// `scope` is the request authority. When it is absent, the daemon uses
/// `Scope::Workspace { path: kiln }`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NoteRef {
    pub kiln: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// One kiln, read under one authority: the request of `kiln.graph` and
/// `note.list`.
///
/// `scope` is the request authority. When it is absent, the daemon uses
/// `Scope::Workspace { path: kiln }`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KilnRef {
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
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct FsListDirRequest {
    /// Absolute path of the root to list inside.
    pub root: String,
    /// Root-relative POSIX path of the directory. Empty lists the root.
    #[serde(default)]
    pub rel_path: String,
    /// Include entries git ignores. The file tree sends `true`.
    #[serde(default)]
    pub show_ignored: bool,
    /// Include dotfiles. `.git` never lists, whatever this says.
    #[serde(default)]
    pub show_hidden: bool,
}

/// One diffset, named by its source: the request of `diff.get` and
/// `diff.comments`.
///
/// For a branch source, an empty `base` asks the daemon for the default
/// branch. The reply then names the branch that the daemon used.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffsetRef {
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
///
/// A key that the type does not know is refused, not dropped. A caller that
/// sends a field of another shape, for example a `session_id`, learns of the
/// mistake. The web route reads this type, so the rule holds at each edge.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffCommentRequest {
    /// The diffset of the file.
    pub source: crate::diff::DiffsetSource,
    /// The absolute path of the root of the file. A session record and a
    /// proposal need it, because each can have more than one root. A branch
    /// source names its own root.
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

/// One comment of the diffset of `source`: the request of
/// `diff.resolve_comment` and `diff.delete_comment`.
///
/// [`crate::diff::CommentRef`] names a comment too, but its wire field is
/// `id`. The two methods here send `comment_id`, so the shapes stay apart.
/// A key that the type does not know is refused, as in [`DiffCommentRequest`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffCommentKey {
    /// The diffset of the comment.
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

/// What `diff.delete_comment` answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffDeleteCommentReply {
    pub diffset: crate::diff::DiffsetId,
    pub comment_id: String,
    pub deleted: bool,
}

/// The two roots that an `fs.*` mutation may name, and the allowlist that
/// each one selects.
///
/// This type describes the wire. The field that carries it stays a `String`,
/// so the daemon refuses an unknown kind in its own sentence. A body
/// rejection would say only "unknown variant". A web test walks an
/// exhaustive match, so a third kind cannot reach the document without a
/// decision about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum FsRootKind {
    /// A registered project, or the workspace folder of a session.
    Project,
    /// A registered kiln.
    Kiln,
}

/// Request for `fs.move`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsMoveRequest {
    /// Absolute path of the root that holds both ends.
    pub root: String,
    /// `"project"` or `"kiln"`. It selects the allowlist that the daemon
    /// checks `root` against.
    #[cfg_attr(feature = "openapi", schema(value_type = FsRootKind))]
    pub kind: String,
    /// Root-relative POSIX path of the entry to move.
    pub from_rel: String,
    /// Root-relative POSIX path that the entry takes.
    pub to_rel: String,
}

/// Request for `fs.mkdir` and `fs.trash`: one path inside one root.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsPathRequest {
    /// Absolute path of the root that holds the entry.
    pub root: String,
    /// `"project"` or `"kiln"`. It selects the allowlist that the daemon
    /// checks `root` against.
    #[cfg_attr(feature = "openapi", schema(value_type = FsRootKind))]
    pub kind: String,
    /// Root-relative POSIX path of the entry.
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScmCloneRequest {
    /// The remote repo: `https://…`, `git@host:…`, or the `owner/repo`
    /// shorthand.
    pub url: String,
    /// Where to put the clone. Absolute, and it must not exist. Absent takes
    /// `[workspace] root_dir/<repo-name>`.
    #[serde(default)]
    pub dest: Option<String>,
    /// The project name of the clone. Absent takes the name in the URL.
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
///
/// `title` and `updated_at` are always written, even when the index holds
/// none: the value is `null`, and the key is never absent. That is why they
/// carry `required = true` under the `openapi` schema.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NoteListRow {
    /// The file stem, or the whole path when the stem is not UTF-8.
    pub name: String,
    /// Kiln-relative, as the index holds it.
    pub path: String,
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub title: Option<String>,
    pub tags: Vec<String>,
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub updated_at: Option<String>,
    /// The note's own frontmatter, filtered daemon-side to what the author
    /// wrote.
    pub properties: std::collections::BTreeMap<String, serde_json::Value>,
}

impl NoteListRow {
    /// The legacy five-field view, for callers that want no properties.
    pub fn into_parts(self) -> (String, String, Option<String>, Vec<String>, Option<String>) {
        (self.name, self.path, self.title, self.tags, self.updated_at)
    }
}

/// One kiln, as `kiln.list` reports it.
///
/// Every key is written on every row, including the two an older reader
/// treated as optional, so this reply carries no absent-versus-false
/// ambiguity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnRow {
    /// Where the kiln lives. This is the one listing whose job is to say so.
    pub path: String,
    /// The registry key — the name every other API call answers to. An open
    /// directory that no entry names carries the empty string, never `null`.
    pub name: String,
    /// Whether the registry answers for this directory, and therefore
    /// whether `name` is a name an attach accepts. A row with `false` must
    /// not be offered in a picker.
    pub registered: bool,
    /// Whether the daemon holds the kiln open right now. A closed row is not
    /// a dead one: the first request that addresses a kiln opens it.
    pub open: bool,
    /// Seconds since the daemon last touched the kiln, or `null` when it
    /// holds it closed. Always written, so `required` rather than optional.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub last_access_secs_ago: Option<u64>,
    /// Whether the kiln path is the top level of a git working tree. A
    /// folder below the top level answers `false`, because `diff.get`
    /// refuses it.
    pub git: bool,
}

/// One wikilink target, as [`NoteByNameReply::wikilinks`] reports it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikilinkTarget {
    pub target: String,
}

/// What `get_note_by_name` answers: the note's own facts, without the heavy
/// fields of the stored [`crate::storage::NoteRecord`] (no embedding
/// vector).
///
/// `links_to` and `wikilinks` carry the same targets under two names: the
/// web reader pins `links_to`, and the RPC client's own DTO reads
/// `wikilinks`. Both stay on the wire, so both are named here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NoteByNameReply {
    /// Relative to the kiln root.
    pub path: String,
    /// The note's title. The daemon writes the empty string when it has
    /// none; this is never `null`.
    pub title: String,
    pub tags: Vec<String>,
    /// Every wikilink target in the note, as written.
    pub links_to: Vec<String>,
    /// The same targets, one object each.
    pub wikilinks: Vec<WikilinkTarget>,
    /// BLAKE3 of the note's content as the index holds it.
    pub content_hash: String,
}

/// One note whose wikilinks point at the note `get_backlinks` resolved.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BacklinkEntry {
    pub name: String,
    /// Kiln-relative, as the index holds it.
    pub path: String,
    /// `null` when the source note declares no title. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub title: Option<String>,
    /// Byte offset of the first link occurrence in the source. Absent — not
    /// `null` — for a span-less legacy index row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_end: Option<i64>,
}

/// What `get_backlinks` answers: the resolved note, and the notes that
/// wikilink to it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GetBacklinksReply {
    pub path: String,
    pub title: String,
    pub backlinks: Vec<BacklinkEntry>,
}

/// One node of the `kiln.graph` note-link graph.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnGraphNote {
    /// Kiln-relative, and the value a resolved link's `target` joins against.
    pub path: String,
    /// Never empty: the daemon falls back to the file stem.
    pub title: String,
    pub tags: Vec<String>,
}

/// One edge of the `kiln.graph` note-link graph.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnGraphLink {
    /// The linking note's path. Always a `path` in [`KilnGraphReply::notes`].
    pub source: String,
    /// The linked note's path when `resolved`; otherwise the target as it
    /// was written, which names no note.
    pub target: String,
    /// Whether `target` resolves to a note the caller can see.
    pub resolved: bool,
}

/// What `kiln.graph` answers: the full note-link graph of a kiln.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnGraphReply {
    pub notes: Vec<KilnGraphNote>,
    pub links: Vec<KilnGraphLink>,
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
