//! Storage and Kiln RPC methods
//!
//! Methods for managing kilns, notes, and storage operations.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;
use std::path::{Path, PathBuf};

use super::{DaemonClient, NO_PARAMS};
use crucible_core::protocol::requests::PathRequest;

use crate::storage::sqlite::FtsResult;

impl DaemonClient {
    /// Apply a text mutation once. An unanswered write must never be replayed blindly.
    pub async fn fs_write(
        &self,
        request: &crucible_core::file_write::FileWriteRequest,
    ) -> Result<serde_json::Value> {
        self.call(RpcMethod::FsWrite, serde_json::to_value(request)?)
            .await
    }

    // `fs_read` had one caller and only built the row's request from a
    // borrowed one; `crucible-web`'s `routes/kiln.rs` now calls
    // `rpc_fs_read` directly (step 19 item 9).

    // =========================================================================
    // Kiln RPC Methods
    // =========================================================================

    pub async fn kiln_open(&self, path: &Path) -> Result<()> {
        self.kiln_open_with_options(path, false, false).await?;
        Ok(())
    }

    pub async fn kiln_open_with_options(
        &self,
        path: &Path,
        process: bool,
        force: bool,
    ) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::KilnOpen,
            KilnOpenRequest {
                path: path.to_string_lossy().to_string(),
                process,
                force,
            },
        )
        .await
    }

    /// Register a directory as a kiln under `name`.
    ///
    /// The daemon is the only writer of `kilns.json`, so every registration
    /// goes through here — including the CLI's, which used to edit the user's
    /// config file itself.
    pub async fn kiln_register(
        &self,
        name: &str,
        path: &Path,
        auto: bool,
        make_default: bool,
    ) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::KilnRegister,
            KilnRegisterRequest {
                name: Some(name.to_string()),
                path: path.to_string_lossy().to_string(),
                auto,
                make_default,
            },
        )
        .await
    }

    // `kiln_register_derived` had one caller
    // (`crates/crucible-cli/src/commands/acp/mod.rs`), which now calls
    // `rpc_kiln_register` with `name: None` directly (step 19 item 9).
    // `llm_register_provider`, `kiln_registry_list`, `kiln_forget` and
    // `kiln_list` are gone the same way: each had a small, fixed set of
    // callers that now call the generated `rpc_<variant>` method with the
    // row's own request type.

    // =========================================================================
    // Search RPC Methods
    // =========================================================================

    /// Embed `text` using the daemon's configured embedding provider.
    ///
    /// Offloads embedding generation so a non-daemon CLI consumer doesn't
    /// need fastembed/ort linked in. The daemon applies its global
    /// enrichment config (the same one every open kiln indexes with) so
    /// query-time vectors match index-time vectors. `kiln_path` is used
    /// to ensure the kiln is open before the call, not to pick the
    /// provider.
    pub async fn embed_query(&self, kiln_path: &Path, text: &str) -> Result<Vec<f32>> {
        let reply = self
            .rpc_embed_query(EmbedQueryRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                text: text.to_string(),
            })
            .await?;
        Ok(reply.vector)
    }

    /// Full-text search over note titles and bodies.
    ///
    /// The counterpart to [`Self::search_vectors`]: this one finds notes that
    /// literally contain the words, which is what a user typing `cru search`
    /// usually means.
    pub async fn search_text(
        &self,
        kiln_path: &Path,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FtsResult>> {
        self.call(
            RpcMethod::SearchText,
            SearchTextRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                query: query.to_string(),
                limit,
            },
        )
        .await
    }

    /// Semantic vector search, block first.
    ///
    /// The daemon answers from the same block-first search the search tool
    /// and precognition use, so a hit names the passage when the kiln has
    /// block rows. `scope` is accepted for older callers; the daemon derives
    /// authority from `kiln_path` alone.
    pub async fn search_vectors(
        &self,
        kiln_path: &Path,
        vector: &[f32],
        limit: usize,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Vec<VectorHit>> {
        self.call(
            RpcMethod::SearchVectors,
            SearchVectorsRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                vector: vector.to_vec(),
                limit,
                scope,
            },
        )
        .await
    }

    // `search_grep` had no caller and is gone; a caller that wants
    // `search_grep` now builds `GrepSearchRequest` and calls
    // `rpc_search_grep` directly (step 19 item 9).

    /// List notes by metadata filter. `scope = None` defaults to the kiln's
    /// workspace authority server-side.
    pub async fn list_notes(
        &self,
        kiln_path: &Path,
        path_filter: Option<&str>,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Vec<NoteListRow>> {
        self.call(
            RpcMethod::ListNotes,
            ListNotesRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                path_filter: path_filter.map(|f| f.to_string()),
                scope,
            },
        )
        .await
    }

    /// Case-insensitive fuzzy lookup by path or title. `scope = None`
    /// defaults to the kiln's workspace authority server-side.
    pub async fn get_note_by_name(
        &self,
        kiln_path: &Path,
        name: &str,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Option<NoteByNameReply>> {
        let result: serde_json::Value = self
            .call(
                RpcMethod::GetNoteByName,
                NoteRef {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    name: name.to_string(),
                    scope,
                },
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }

    /// Resolve a note by name and return the notes that wikilink to it.
    ///
    /// Returns `None` when the name resolves to no note.
    pub async fn get_backlinks(
        &self,
        kiln_path: &Path,
        name: &str,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Option<GetBacklinksReply>> {
        let result: serde_json::Value = self
            .call(
                RpcMethod::GetBacklinks,
                NoteRef {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    name: name.to_string(),
                    scope,
                },
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }

    /// The full note-link graph for a kiln. `scope = None` defaults to the
    /// kiln's workspace authority server-side. Returns the raw
    /// `{ notes: [...], links: [...] }` value verbatim.
    pub async fn kiln_graph(
        &self,
        kiln_path: &Path,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<KilnGraphReply> {
        self.call(
            RpcMethod::KilnGraph,
            KilnRef {
                kiln: kiln_path.to_string_lossy().to_string(),
                scope,
            },
        )
        .await
    }

    /// Detect unlinked mentions of existing notes in `text`.
    ///
    /// Returns the raw `[{ mention, target, offset }]` suggestion array.
    pub async fn suggest_links(
        &self,
        kiln_path: &Path,
        text: &str,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Vec<crate::tools::autolink::LinkSuggestion>> {
        let reply: crate::tools::autolink::SuggestLinksReply = self
            .call(
                RpcMethod::SuggestLinks,
                SuggestLinksRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    text: text.to_string(),
                    scope,
                },
            )
            .await?;

        Ok(reply.suggestions)
    }

    // =========================================================================
    // NoteStore RPC Methods
    // =========================================================================

    pub async fn note_upsert(
        &self,
        kiln_path: &Path,
        note: &crucible_core::storage::NoteRecord,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                RpcMethod::NoteUpsert,
                NoteUpsertRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    note: serde_json::to_value(note)?,
                },
            )
            .await?;
        Ok(())
    }

    // `note_get` (the `scope: None` default of `note_get_scoped`) had no
    // caller and is gone (step 19 item 9).

    /// Read one note, decoding the row's `null`-or-record reply into
    /// `None`/`Some`. Used by the `NoteStore` backend that reads through the
    /// daemon (`crates/crucible-daemon/src/rpc_client/storage.rs`).
    pub async fn note_get_scoped(
        &self,
        kiln_path: &Path,
        path: &str,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Option<crucible_core::storage::NoteRecord>> {
        let result: serde_json::Value = self
            .call(
                RpcMethod::NoteGet,
                NotePathRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    path: path.to_string(),
                    scope,
                },
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            let note: crucible_core::storage::NoteRecord = serde_json::from_value(result)?;
            Ok(Some(note))
        }
    }

    pub async fn note_delete(&self, kiln_path: &Path, path: &str) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                RpcMethod::NoteDelete,
                NotePathRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    path: path.to_string(),
                    scope: None,
                },
            )
            .await?;
        Ok(())
    }

    // `note_list` (the `scope: None` default of `note_list_scoped`) had no
    // caller and is gone (step 19 item 9).

    /// List a kiln's notes, scoped. Used by the same `NoteStore` backend as
    /// [`Self::note_get_scoped`].
    pub async fn note_list_scoped(
        &self,
        kiln_path: &Path,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Vec<crucible_core::storage::NoteRecord>> {
        self.call(
            RpcMethod::NoteList,
            KilnRef {
                kiln: kiln_path.to_string_lossy().to_string(),
                scope,
            },
        )
        .await
    }

    // =========================================================================
    // Pipeline RPC Methods
    // =========================================================================

    /// Turns `&[PathBuf]` into the row's wire `Vec<String>`, and its typed
    /// reply into the `(processed, skipped, errors)` tuple every caller
    /// wants.
    pub async fn process_batch(
        &self,
        kiln_path: &Path,
        file_paths: &[PathBuf],
    ) -> Result<(usize, usize, Vec<(String, String)>)> {
        let paths: Vec<String> = file_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let reply = self
            .rpc_process_batch(ProcessBatchRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                paths,
            })
            .await?;

        let errors = reply
            .errors
            .into_iter()
            .map(|e| (e.path, e.error))
            .collect();
        Ok((reply.processed, reply.skipped, errors))
    }

    // `storage_verify`, `storage_cleanup`, `storage_backup` and
    // `storage_restore` each had one caller
    // (`crates/crucible-cli/src/commands/storage.rs`) and only built the
    // row's request from a `Path`; that caller now calls the generated
    // `rpc_storage_*` method directly (step 19 item 9).
    //
    // `mcp_start` and `mcp_stop` are gone the same way — their one caller
    // (`crates/crucible-cli/src/commands/mcp.rs`) now calls
    // `rpc_mcp_start`/`rpc_mcp_stop` directly. `mcp_status` had no caller and
    // is gone too.

    /// Turn one verified webhook delivery into a `webhook:received` event.
    ///
    /// The ingress route has already checked the signature over the bytes as
    /// received, so `body` reaches the daemon as the sender wrote it.
    pub async fn webhook_receive(
        &self,
        name: String,
        headers: std::collections::HashMap<String, String>,
        body: String,
    ) -> Result<crucible_core::protocol::requests::WebhookReceiveReply> {
        self.call(
            RpcMethod::WebhookReceive,
            WebhookReceiveRequest {
                name,
                headers: headers
                    .into_iter()
                    .map(|(key, value)| (key, serde_json::Value::String(value)))
                    .collect(),
                body,
            },
        )
        .await
    }

    // =========================================================================
    // Project RPC Methods
    // =========================================================================

    /// Register `path` for a local caller: the CLI, the TUI, a Lua script.
    pub async fn project_register(&self, path: &Path) -> Result<crucible_core::Project> {
        self.call(
            RpcMethod::ProjectRegister,
            crucible_core::protocol::requests::ProjectRegisterRequest {
                path: path.to_string_lossy().to_string(),
                untrusted: false,
            },
        )
        .await
    }

    /// Register `path` for a caller that is not the local user at the
    /// machine. The web API is the only caller of this today: it refuses a
    /// credential store or the user's config/state tree on top of the floor
    /// every caller gets, because a registered root becomes a read scope for
    /// every client afterward.
    pub async fn project_register_untrusted(&self, path: &Path) -> Result<crucible_core::Project> {
        self.call(
            RpcMethod::ProjectRegister,
            crucible_core::protocol::requests::ProjectRegisterRequest {
                path: path.to_string_lossy().to_string(),
                untrusted: true,
            },
        )
        .await
    }

    /// Open the kilns of the project rooted at `path`, if one is registered.
    ///
    /// The matching and the opening both happen daemon-side: it is the only
    /// layer holding both the project registry and the kiln registry.
    pub async fn project_open_kilns(&self, path: &Path) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::ProjectOpenKilns,
            PathRequest {
                path: path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn project_unregister(&self, path: &Path) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                RpcMethod::ProjectUnregister,
                PathRequest {
                    path: path.to_string_lossy().to_string(),
                },
            )
            .await?;
        Ok(())
    }

    // `project_list` had no caller and is gone; a caller now calls
    // `rpc_project_list(())` directly (step 19 item 9).

    /// Every project name Crucible knows, with the layer that owns it.
    ///
    /// Not [`Self::project_list`], which answers "what is registered". This is
    /// the two-layer view: `[projects.*]` the user authored beside
    /// `projects.json` the daemon wrote.
    pub async fn project_registry_list(&self) -> Result<serde_json::Value> {
        self.call(RpcMethod::ProjectRegistryList, NO_PARAMS).await
    }

    // `fs_list_dir` had no caller and is gone; a caller now builds
    // `FsListDirRequest` and calls `rpc_fs_list_dir` directly (step 19 item
    // 9).

    /// `diff.get`: the files of one diffset, with counts and no text.
    ///
    /// A read, so the client retries it.
    pub async fn diff_get(
        &self,
        source: &crucible_core::diff::DiffsetSource,
    ) -> Result<crucible_core::diff::Diffset> {
        self.call_with_retry(
            RpcMethod::DiffGet,
            DiffsetRef {
                source: source.clone(),
            },
        )
        .await
    }

    /// `diff.file`: the two texts of one file of a diffset, with no `root`
    /// (a session record source needs `root`, because a session can have
    /// more than one root; a caller that has one builds `DiffFileRequest`
    /// and calls `rpc_diff_file` directly).
    ///
    /// `from` is the old path of a renamed file. A read, so the client
    /// retries it.
    pub async fn diff_file(
        &self,
        source: &crucible_core::diff::DiffsetSource,
        path: &str,
        from: Option<&str>,
    ) -> Result<crucible_core::diff::DiffFileText> {
        self.call_with_retry(
            RpcMethod::DiffFile,
            DiffFileRequest {
                source: source.clone(),
                path: path.to_string(),
                from: from.map(str::to_string),
                root: None,
            },
        )
        .await
    }

    /// `diff.comment`: anchor a comment to a line range of one file.
    ///
    /// A write, so the client sends it once.
    pub async fn diff_comment(&self, request: DiffCommentRequest) -> Result<DiffCommentReply> {
        self.call(RpcMethod::DiffComment, request).await
    }

    // `diff_resolve_comment` and `diff_delete_comment` had no caller and are
    // gone; a caller now builds `DiffCommentKey` and calls
    // `rpc_diff_resolve_comment`/`rpc_diff_delete_comment` directly (step 19
    // item 9).

    /// `diff.comments`: the comments of a diffset, each projected onto the
    /// current text of its side.
    ///
    /// A read, so the client retries it.
    pub async fn diff_comments(
        &self,
        source: &crucible_core::diff::DiffsetSource,
    ) -> Result<DiffCommentsReply> {
        self.call_with_retry(
            RpcMethod::DiffComments,
            DiffsetRef {
                source: source.clone(),
            },
        )
        .await
    }

    // `fs_move`, `fs_mkdir` and `fs_trash` had no caller and are gone; a
    // caller now builds `FsMoveRequest`/`FsPathRequest` and calls
    // `rpc_fs_move`/`rpc_fs_mkdir`/`rpc_fs_trash` directly (step 19 item 9).

    /// Clone a remote git repo and register it as a project.
    ///
    /// `dest` (absolute, must not exist) overrides the configured
    /// `[workspace] root_dir/<repo-name>` default; `name` overrides the repo-name
    /// derived from the URL. Uses a generous 10-minute timeout — cloning a
    /// large repository can far exceed the default request timeout.
    pub async fn scm_clone(
        &self,
        url: &str,
        dest: Option<&Path>,
        name: Option<&str>,
    ) -> Result<crate::scm::ScmCloneResponse> {
        self.call_with_timeout(
            RpcMethod::ScmClone,
            ScmCloneRequest {
                url: url.to_string(),
                dest: dest.map(|p| p.to_string_lossy().to_string()),
                name: name.map(str::to_string),
            },
            std::time::Duration::from_secs(600),
        )
        .await
    }

    // `project_get` had no caller and is gone; a caller now builds
    // `PathRequest` and calls `rpc_project_get` directly — the row already
    // answers `Option<Project>`, so no null-check decode is needed (step 19
    // item 9).
}
