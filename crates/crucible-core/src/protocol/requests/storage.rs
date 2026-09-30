//! Wire types of the `storage` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

/// Request for `kiln.open`.
///
/// `process` and `force` default because the server read them with
/// `optional_param!`: a caller that sends only `path` must keep working.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotePathRequest {
    pub kiln: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::storage::Scope>,
}

/// Request for `process_batch`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProcessBatchRequest {
    pub kiln: String,
    pub paths: Vec<String>,
}

/// Request for `storage.backup`.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StorageBackupRequest {
    pub kiln: String,
    pub dest: String,
}

/// Request for `storage.restore`.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchTextRequest {
    pub kiln: String,
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

/// Request for `embed.query`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffsetRef {
    pub source: crate::diff::DiffsetSource,
}

/// Request for `diff.file`: one file of the diffset of `source`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
/// This type describes the wire, and the field that carries it is this type,
/// not a `String`: an unknown kind now fails at deserialize, where the
/// document's own error names the bad value, before a handler ever sees the
/// request. A web test still walks an exhaustive match over the two
/// variants, so a third kind cannot reach the document without a decision
/// about it.
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
    /// Selects the allowlist that the daemon checks `root` against.
    pub kind: FsRootKind,
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
    /// Selects the allowlist that the daemon checks `root` against.
    pub kind: FsRootKind,
    /// Root-relative POSIX path of the entry.
    pub rel_path: String,
}

/// Request for `note.rename` (and its `note.move` alias).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProcessFileRequest {
    pub kiln: String,
    pub path: String,
}

/// Request for `list_notes`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// One file a batch or a full-kiln process step failed on: the shape
/// `kiln.open { process: true }` and `process_batch` both answer for an
/// unindexed file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FileProcessError {
    pub path: String,
    pub error: String,
}

/// What `kiln.open` answers.
///
/// Untagged: the three arms answer a caller that asked to index the kiln
/// (`Processed`), one whose indexing failed outright (`ProcessError`), and
/// one that only opened the kiln (`Opened`). The wire has always told the
/// three apart by which keys are present, not by a tag field.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum KilnOpenReply {
    /// The kiln opened and its indexing pass finished.
    Processed {
        status: String,
        discovered: usize,
        processed: usize,
        skipped: usize,
        errors: Vec<FileProcessError>,
    },
    /// The kiln opened, but indexing raised an error.
    ProcessError {
        status: String,
        process_error: String,
    },
    /// The kiln opened; no indexing was requested.
    Opened { status: String },
}

/// A bare status word: what `kiln.close`, `note.delete` and `fs.mkdir`'s
/// siblings answer when there is nothing else to say. The word itself
/// varies (`"ok"`, `"not_found"`), so it stays a `String`, not a `bool`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StatusReply {
    pub status: String,
}

/// Whether `kiln.register` added a new state entry or found one already
/// there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum KilnRegisterOutcome {
    Added,
    AlreadyPresent,
}

/// What `kiln.register` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnRegisterReply {
    pub status: String,
    pub name: String,
    pub path: String,
    pub outcome: KilnRegisterOutcome,
    pub state_file: String,
}

/// What `kiln.forget` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnForgetReply {
    pub status: String,
    pub name: String,
    pub state_file: String,
    /// Always `"next daemon start"`: a removal waits for the boot freeze,
    /// unlike an add, which takes effect immediately.
    pub takes_effect: String,
}

/// What `llm.register_provider` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmRegisterProviderReply {
    pub status: String,
    pub provider: String,
    pub model: String,
    pub state_file: String,
    /// `"additive"` or `"deferred"` — see `SelectionOutcome` (daemon-local:
    /// it also carries the apply-now decision, not only the wire word).
    pub outcome: String,
    /// Whether the running daemon applied the selection immediately.
    pub live: bool,
    /// What the daemon keeps using until the next start, when `live` is
    /// `false`. `null` — not absent — when nothing was serving, or the
    /// selection applied: the wire has always written this key.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub still_serving: Option<String>,
}

/// A full-text search result: the reply shape of `search_text`.
///
/// Also the `search_text` wire shape: the daemon serializes it and the
/// client deserializes it, so the field names are the JSON keys.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FtsResult {
    /// Path to the note
    pub path: String,
    /// Note title
    pub title: String,
    /// Snippet of matching content (with highlights). An older daemon may
    /// omit it.
    #[serde(default)]
    pub snippet: String,
    /// BM25 relevance score (lower is better in FTS5)
    pub rank: f64,
}

/// A single content-search hit: one row of `search_grep`.
///
/// `match_start`/`match_end` are **character** offsets into `text`
/// (post-trim), suitable for `<mark>` highlighting in the web UI. Only the
/// first match on a line is reported. Wire keys (`path`/`rel_path`/`line`/
/// `text`/`match_start`/`match_end`) are the `search_grep` RPC + `POST
/// /api/search/grep` contract.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GrepHit {
    /// Absolute path to the matched file.
    pub path: String,
    /// Path relative to the search's `rel_base` (forward-slash separators).
    pub rel_path: String,
    /// 1-based line number.
    pub line: u64,
    /// The matched line, trimmed of surrounding whitespace and capped at a
    /// daemon-side length.
    pub text: String,
    /// Character offset of the first match's start within `text`.
    pub match_start: usize,
    /// Character offset of the first match's end within `text`.
    pub match_end: usize,
}

/// Result of a `search_grep` call: the hits plus whether they were capped at
/// the requested limit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GrepSearchResponse {
    pub hits: Vec<GrepHit>,
    pub truncated: bool,
}

/// What `embed.query` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmbedQueryReply {
    pub vector: Vec<f32>,
}

/// What `note.upsert` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NoteUpsertReply {
    pub status: String,
    pub events_count: usize,
}

/// What `process_file` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProcessFileReply {
    /// `"processed"` or `"skipped"`.
    pub status: String,
    pub path: String,
}

/// What `process_batch` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProcessBatchReply {
    pub processed: usize,
    pub skipped: usize,
    pub errors: Vec<FileProcessError>,
}

/// What `project.register` takes.
///
/// `untrusted` tells the daemon the caller is not the local user at the
/// machine — today, the web API sets it. A local caller (the CLI, the TUI, a
/// Lua script) already has full filesystem access to whatever it can name, so
/// it omits the field and gets the plain daemon floor (the filesystem root,
/// the home directory and the system trees, refused for every caller). An
/// untrusted caller also gets the extra refusal of a personal
/// credential store or the user's config/state tree, because a registered
/// root is a read scope for every client afterward.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectRegisterRequest {
    /// Absolute path of the project root.
    pub path: String,
    /// The caller is not the local user at the machine.
    #[serde(default)]
    pub untrusted: bool,
}

/// One kiln `project.open_kilns` opened.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OpenedKiln {
    /// `null` — not absent — for a kiln the registry cannot name: the wire
    /// has always written this key.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub kiln: Option<String>,
    pub path: String,
}

/// One kiln `project.open_kilns` left closed because it is lazy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkippedKiln {
    /// `null` — not absent — for a kiln the registry cannot name.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub kiln: Option<String>,
    pub reason: String,
}

/// One kiln `project.open_kilns` failed to open.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KilnOpenError {
    /// `null` — not absent — for a kiln the registry cannot name.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub kiln: Option<String>,
    pub path: String,
    pub error: String,
}

/// What `project.open_kilns` answers.
///
/// Untagged: a directory that matches no registered project answers with
/// `NoMatch` alone — no `project` key, no `errors` key — and a match answers
/// with `Matched`, which always carries both. The wire has told the two
/// apart by which keys are present since the method shipped.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProjectOpenKilnsReply {
    Matched {
        matched: bool,
        project: String,
        opened: Vec<OpenedKiln>,
        skipped: Vec<SkippedKiln>,
        errors: Vec<KilnOpenError>,
    },
    /// Tried last, and a newtype around a `deny_unknown_fields` struct:
    /// `Matched`'s extra keys (`project`, `errors`) would otherwise
    /// deserialize into this arm too, since a struct silently ignores a
    /// field it does not name (serde has no per-variant
    /// `deny_unknown_fields` on an untagged enum, only a container-level
    /// one, so the closed struct lives apart from the variant).
    NoMatch(ProjectOpenKilnsNoMatch),
}

/// The `NoMatch` arm of [`ProjectOpenKilnsReply`], closed so it cannot also
/// read a `Matched` payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectOpenKilnsNoMatch {
    pub matched: bool,
    pub opened: Vec<OpenedKiln>,
    pub skipped: Vec<SkippedKiln>,
}

/// What `scm.clone` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScmCloneResponse {
    /// Absolute path of the freshly cloned repository.
    pub path: String,
    /// The `Project` registered for the clone.
    pub project: crate::project::Project,
}

/// One directory entry in an `fs.list_dir` response.
///
/// Wire keys (`name`/`rel_path`/`is_dir`/`size`/`modified`/`status`) are
/// byte-identical to the TypeScript `FsEntry`. `status` is a Phase-1
/// decoration seam and is always `None`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix epoch seconds, or `null` when the platform cannot report it.
    /// Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub modified: Option<u64>,
    /// The git/diff decoration seam. Always `null` today, and deliberately
    /// open: whatever fills it will not be a string.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub status: Option<serde_json::Value>,
}

/// One directory level, plus whether the cap cut it short: what
/// `fs.list_dir` answers.
///
/// Both keys are part of the cross-language contract (TypeScript
/// `FsListing`). The response used to be a bare array, which had nowhere to
/// say "there is more" — so a capped listing would have been
/// indistinguishable from a complete one, which is worse than the slow
/// response it replaces.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsListing {
    pub entries: Vec<FsEntry>,
    pub truncated: bool,
}

/// Why one inbound reference was left as it was, by an `fs.move` or a
/// `note.rename` that rewrote links.
///
/// A closed set, because the browser prints a sentence per reason. It was
/// four string literals spelled in five places, so a fifth reason could
/// reach a client whose reader has no arm for it. A daemon test walks an
/// exhaustive match over the two clients, so a new variant fails to compile
/// until somebody decides what each says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SkipReason {
    /// The stem is shared by several notes, so no single target is meant.
    Ambiguous,
    /// The file bytes no longer match the index; a reindex catches up.
    StaleSpan,
    /// A canvas resolves to the target but stores it under another spelling.
    CanvasNoExactMatch,
    /// The canvas could not be read.
    CanvasUnreadable,
}

/// One inbound reference that an `fs.move` or a `note.rename` intentionally
/// left untouched.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkippedRef {
    pub source_path: String,
    pub raw_target: String,
    pub reason: SkipReason,
}

/// What `fs.move` answers.
///
/// The two link-report keys are absent for a move the link index does not
/// watch — a directory, an asset, a project file — which is the shape the
/// browser already reads.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsMoveReply {
    /// Always true. A move that did not happen is an error, not a `false`.
    pub moved: bool,
    /// Sources whose inbound links were rewritten. Kiln note and canvas
    /// moves only. Absent, never null, for a move with nothing to report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub rewritten_sources: Option<Vec<String>>,
    /// Inbound links left as they were, with the reason for each. Absent,
    /// never null, for a move with nothing to report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub skipped: Option<Vec<SkippedRef>>,
}

/// What `fs.mkdir` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsMkdirReply {
    /// Always true. A refusal is an error, not a `false`.
    pub created: bool,
}

/// What `fs.trash` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FsTrashReply {
    /// Always true. A refusal is an error, not a `false`.
    pub trashed: bool,
    /// Where the entry now sits, RELATIVE to the root it was trashed from.
    pub trash_path: String,
}

/// Outcome of a `note.rename` / `note.move`, returned to the caller for UX
/// ("N links updated, M ambiguous links skipped").
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NoteRenameReply {
    pub from: String,
    pub to: String,
    pub rewritten_sources: Vec<String>,
    pub skipped: Vec<SkippedRef>,
}

/// What `storage.verify`, `storage.cleanup`, `storage.backup` and
/// `storage.restore` answer today: none of the four is implemented yet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NotImplementedReply {
    pub status: String,
    pub message: String,
}

/// What `mcp.start` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpStartReply {
    pub status: String,
    pub transport: String,
    /// The SSE port, or `null` under stdio. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub port: Option<u16>,
    pub tool_count: usize,
}

/// The running arm of [`McpStatus`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpRunning {
    /// Always `true`.
    pub running: bool,
    /// Transport type: `sse` or `stdio`.
    pub transport: String,
    /// The SSE port, or `null` under stdio. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub port: Option<u16>,
    /// The kiln path the server serves.
    pub kiln_path: String,
    /// Whether the server task has already finished, which is how a crashed
    /// server reads while the manager still calls itself running.
    pub finished: bool,
}

/// The stopped arm of [`McpStatus`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpStopped {
    /// Always `false`.
    pub running: bool,
}

/// What `mcp.status` answers: the server is up, or it is not.
///
/// Untagged, because the two arms are told apart by `running` and the wire
/// has always spelled them that way. The stopped arm carries `running`
/// ALONE: a stopped server has no transport, no port and no kiln, and
/// writing those keys as null would say it has them and they are empty.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum McpStatus {
    /// A server is serving a kiln. Listed first so a payload that carries
    /// the running keys never reads as the stopped arm, which ignores them.
    Running(McpRunning),
    /// No server is running.
    Stopped(McpStopped),
}

#[cfg(test)]
mod mcp_status_tests {
    use super::*;

    /// The untagged union reads BOTH ways: each arm's wire form comes back as
    /// that arm, so the running payload can never be read as a stopped one.
    ///
    /// Moved from `crates/crucible-web/src/routes/mcp.rs` when
    /// `GET /api/mcp/status` was deleted (Simplification Plan step 19): this
    /// property belongs to the type, not to the route that used to forward it.
    #[test]
    fn each_status_arm_round_trips_as_itself() {
        let running = McpStatus::Running(McpRunning {
            running: true,
            transport: "sse".to_string(),
            port: Some(3847),
            kiln_path: "/kilns/docs".to_string(),
            finished: false,
        });
        let wire = serde_json::to_value(&running).expect("the running arm serialises");
        assert_eq!(wire["running"], serde_json::json!(true));
        assert_eq!(wire["port"], serde_json::json!(3847));
        assert_eq!(
            serde_json::from_value::<McpStatus>(wire).expect("the running arm parses"),
            running
        );

        let stopped = McpStatus::Stopped(McpStopped { running: false });
        let wire = serde_json::to_value(&stopped).expect("the stopped arm serialises");
        assert_eq!(wire, serde_json::json!({ "running": false }));
        assert_eq!(
            serde_json::from_value::<McpStatus>(wire).expect("the stopped arm parses"),
            stopped
        );
    }

    /// A stdio server has no port, and says so with a written null rather
    /// than by leaving the key out: "no port" and "this answer does not
    /// mention ports" are different sentences.
    #[test]
    fn a_stdio_server_writes_a_null_port() {
        let wire = serde_json::to_value(McpStatus::Running(McpRunning {
            running: true,
            transport: "stdio".to_string(),
            port: None,
            kiln_path: "/kilns/docs".to_string(),
            finished: false,
        }))
        .expect("the running arm serialises");
        assert!(wire.get("port").is_some(), "{wire}");
        assert!(wire["port"].is_null(), "{wire}");
    }
}

/// What `webhook.receive` answers.
///
/// Acceptance only: the delivery became a `webhook:received` event, and
/// whether a plugin was listening is not this answer's business. Every
/// refusal is an HTTP error at the ingress route, which never reaches this
/// RPC.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WebhookReceiveReply {
    /// Always `ok`.
    pub status: String,
}

/// A single suggestion to convert a plain-text mention into a wikilink:
/// one row of `suggest_links`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LinkSuggestion {
    /// The text that was found as a mention (preserves original casing)
    pub mention: String,
    /// The note name to link to
    pub target: String,
    /// Byte offset in the text where the mention starts
    pub offset: usize,
}

/// What `suggest_links` answers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SuggestLinksReply {
    pub suggestions: Vec<LinkSuggestion>,
}
