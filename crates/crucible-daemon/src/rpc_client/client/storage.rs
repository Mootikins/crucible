//! Storage and Kiln RPC methods
//!
//! Methods for managing kilns, notes, and storage operations.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;
use std::path::{Path, PathBuf};

use super::{DaemonClient, NO_PARAMS};
use crucible_core::protocol::requests::{KilnPathRequest, NameRequest, PathRequest};

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

    /// Read one file through the daemon's enclosing-root rule. The answer is
    /// a [`crucible_core::file_write::FileReadReply`] with `"ok": true`, or a
    /// refusal with `"ok": false` and a `failure` kind, as `fs.write` answers.
    pub async fn fs_read(
        &self,
        request: &crucible_core::file_write::FileReadRequest,
    ) -> Result<serde_json::Value> {
        self.call(RpcMethod::FsRead, serde_json::to_value(request)?)
            .await
    }

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
        self.kiln_register_opt(Some(name), path, auto, make_default)
            .await
    }

    /// Register a directory, letting the daemon derive the name.
    ///
    /// For `--kiln <path>` and kiln discovery, where the user named a
    /// directory and not a name. The derivation belongs to the daemon because
    /// it depends on what is already registered: a caller that derived its own
    /// would derive against a different set and pick a name the daemon then
    /// refuses.
    pub async fn kiln_register_derived(
        &self,
        path: &Path,
        auto: bool,
        make_default: bool,
    ) -> Result<serde_json::Value> {
        self.kiln_register_opt(None, path, auto, make_default).await
    }

    async fn kiln_register_opt(
        &self,
        name: Option<&str>,
        path: &Path,
        auto: bool,
        make_default: bool,
    ) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::KilnRegister,
            KilnRegisterRequest {
                name: name.map(str::to_string),
                path: path.to_string_lossy().to_string(),
                auto,
                make_default,
            },
        )
        .await
    }

    /// Record which LLM provider and model to use.
    ///
    /// The daemon owns `<data_home>/llm.json`. The reply's `live` says whether
    /// the running daemon took the selection now or whether it waits for the
    /// next start, and `still_serving` names what it keeps using until then.
    pub async fn llm_register_provider(
        &self,
        provider: &str,
        model: &str,
        make_default: bool,
    ) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::LlmRegisterProvider,
            LlmRegisterProviderRequest {
                provider: provider.to_string(),
                model: model.to_string(),
                make_default,
            },
        )
        .await
    }

    /// Every kiln name Crucible knows, with the layer that owns it.
    ///
    /// Not [`Self::kiln_list`], which lists the kilns that happen to be OPEN.
    /// This is the registry: what a session may name.
    pub async fn kiln_registry_list(&self) -> Result<serde_json::Value> {
        self.call(RpcMethod::KilnRegistryList, NO_PARAMS).await
    }

    /// Remove one registration from the daemon's state store.
    pub async fn kiln_forget(&self, name: &str) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::KilnForget,
            NameRequest {
                name: name.to_string(),
            },
        )
        .await
    }

    pub async fn kiln_list(&self) -> Result<Vec<KilnRow>> {
        self.call(RpcMethod::KilnList, NO_PARAMS).await
    }

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
        let result: serde_json::Value = self
            .call(
                RpcMethod::EmbedQuery,
                EmbedQueryRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    text: text.to_string(),
                },
            )
            .await?;

        let vector = result
            .get("vector")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("embed.query response missing vector field"))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();
        Ok(vector)
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

    /// Ripgrep-style content search over `root` (which must be inside a
    /// registered project or open kiln — the daemon enforces containment).
    /// `regex` switches `query` from literal substring to regex matching.
    /// Returns the hits plus whether they were capped at `limit`.
    pub async fn search_grep(
        &self,
        root: &str,
        query: &str,
        regex: bool,
        glob: Option<&str>,
        limit: usize,
        case_insensitive: bool,
    ) -> Result<crate::GrepSearchResponse> {
        self.call(
            RpcMethod::SearchGrep,
            GrepSearchRequest {
                root: root.to_string(),
                query: query.to_string(),
                regex,
                glob: glob.map(str::to_string),
                limit,
                case_insensitive,
            },
        )
        .await
    }

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

    pub async fn note_get(
        &self,
        kiln_path: &Path,
        path: &str,
    ) -> Result<Option<crucible_core::storage::NoteRecord>> {
        self.note_get_scoped(kiln_path, path, None).await
    }

    /// Scope-aware variant of [`Self::note_get`].
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

    pub async fn note_list(
        &self,
        kiln_path: &Path,
    ) -> Result<Vec<crucible_core::storage::NoteRecord>> {
        self.note_list_scoped(kiln_path, None).await
    }

    /// Scope-aware variant of [`Self::note_list`].
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

    pub async fn process_batch(
        &self,
        kiln_path: &Path,
        file_paths: &[PathBuf],
    ) -> Result<(usize, usize, Vec<(String, String)>)> {
        let paths: Vec<String> = file_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let result: serde_json::Value = self
            .call(
                RpcMethod::ProcessBatch,
                ProcessBatchRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    paths,
                },
            )
            .await?;

        let processed = result
            .get("processed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let skipped = result.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let errors: Vec<(String, String)> = result
            .get("errors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| {
                        let path = e.get("path")?.as_str()?.to_string();
                        let error = e.get("error")?.as_str()?.to_string();
                        Some((path, error))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok((processed, skipped, errors))
    }

    // =========================================================================
    // Storage Maintenance RPC Methods (stubs)
    // =========================================================================

    pub async fn storage_verify(&self, kiln_path: &Path) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::StorageVerify,
            KilnPathRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn storage_cleanup(&self, kiln_path: &Path) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::StorageCleanup,
            KilnPathRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn storage_backup(&self, kiln_path: &Path, dest: &Path) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::StorageBackup,
            StorageBackupRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                dest: dest.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn storage_restore(
        &self,
        kiln_path: &Path,
        source: &Path,
    ) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::StorageRestore,
            StorageRestoreRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
                source: source.to_string_lossy().to_string(),
            },
        )
        .await
    }

    // =========================================================================
    // MCP Server RPC Methods
    // =========================================================================

    /// Start the daemon-managed MCP server.
    ///
    /// Spawns an MCP server exposing Crucible's tools for the given kiln.
    /// Supports SSE (default) and stdio transports.
    pub async fn mcp_start(
        &self,
        kiln_path: &str,
        transport: Option<&str>,
        port: Option<u16>,
        no_just: bool,
        just_dir: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.call(
            RpcMethod::McpStart,
            McpStartRequest {
                kiln_path: kiln_path.to_string(),
                no_just,
                transport: transport.map(|t| t.to_string()),
                port,
                just_dir: just_dir.map(|d| d.to_string()),
            },
        )
        .await
    }

    /// Stop the daemon-managed MCP server.
    pub async fn mcp_stop(&self) -> Result<serde_json::Value> {
        self.call(RpcMethod::McpStop, NO_PARAMS).await
    }

    /// Get the status of the daemon-managed MCP server.
    pub async fn mcp_status(&self) -> Result<crate::McpStatus> {
        self.call(RpcMethod::McpStatus, NO_PARAMS).await
    }

    /// Turn one verified webhook delivery into a `webhook:received` event.
    ///
    /// The ingress route has already checked the signature over the bytes as
    /// received, so `body` reaches the daemon as the sender wrote it.
    pub async fn webhook_receive(
        &self,
        name: String,
        headers: std::collections::HashMap<String, String>,
        body: String,
    ) -> Result<crate::WebhookReceiveReply> {
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

    pub async fn project_register(&self, path: &Path) -> Result<crucible_core::Project> {
        self.call(
            RpcMethod::ProjectRegister,
            PathRequest {
                path: path.to_string_lossy().to_string(),
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

    pub async fn project_list(&self) -> Result<Vec<crucible_core::Project>> {
        self.call_with_retry(RpcMethod::ProjectList, NO_PARAMS)
            .await
    }

    /// Every project name Crucible knows, with the layer that owns it.
    ///
    /// Not [`Self::project_list`], which answers "what is registered". This is
    /// the two-layer view: `[projects.*]` the user authored beside
    /// `projects.json` the daemon wrote.
    pub async fn project_registry_list(&self) -> Result<serde_json::Value> {
        self.call(RpcMethod::ProjectRegistryList, NO_PARAMS).await
    }

    /// List one directory level inside a registered project. Read-only,
    /// metadata only.
    ///
    /// Returns the listing envelope verbatim — `{ entries, truncated }`. It was
    /// an unwrapped array; the flag has to survive to the client, or a directory
    /// cut short by the per-entry cap is indistinguishable from a complete one.
    pub async fn fs_list_dir(
        &self,
        root: &str,
        rel_path: &str,
        show_ignored: bool,
        show_hidden: bool,
    ) -> Result<crate::FsListing> {
        self.call(
            RpcMethod::FsListDir,
            FsListDirRequest {
                root: root.to_string(),
                rel_path: rel_path.to_string(),
                show_ignored,
                show_hidden,
            },
        )
        .await
    }

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

    /// `diff.file`: the two texts of one file of a diffset.
    ///
    /// `from` is the old path of a renamed file. A read, so the client
    /// retries it.
    pub async fn diff_file(
        &self,
        source: &crucible_core::diff::DiffsetSource,
        path: &str,
        from: Option<&str>,
    ) -> Result<crucible_core::diff::DiffFileText> {
        self.diff_file_request(DiffFileRequest {
            source: source.clone(),
            path: path.to_string(),
            from: from.map(str::to_string),
            root: None,
        })
        .await
    }

    /// `diff.file` with the whole request. A session record source needs
    /// `root`, because a session can have more than one root.
    pub async fn diff_file_request(
        &self,
        request: DiffFileRequest,
    ) -> Result<crucible_core::diff::DiffFileText> {
        self.call_with_retry(RpcMethod::DiffFile, request).await
    }

    /// `diff.comment`: anchor a comment to a line range of one file.
    ///
    /// A write, so the client sends it once.
    pub async fn diff_comment(&self, request: DiffCommentRequest) -> Result<DiffCommentReply> {
        self.call(RpcMethod::DiffComment, request).await
    }

    /// `diff.resolve_comment`: mark one comment of a diffset resolved.
    ///
    /// A write, so the client sends it once.
    pub async fn diff_resolve_comment(
        &self,
        source: &crucible_core::diff::DiffsetSource,
        comment_id: &str,
    ) -> Result<DiffResolveCommentReply> {
        self.call(
            RpcMethod::DiffResolveComment,
            DiffCommentKey {
                source: source.clone(),
                comment_id: comment_id.to_string(),
            },
        )
        .await
    }

    /// `diff.delete_comment`: remove one comment of a diffset from the store.
    ///
    /// A write, so the client sends it once.
    pub async fn diff_delete_comment(
        &self,
        source: &crucible_core::diff::DiffsetSource,
        comment_id: &str,
    ) -> Result<DiffDeleteCommentReply> {
        self.call(
            RpcMethod::DiffDeleteComment,
            DiffCommentKey {
                source: source.clone(),
                comment_id: comment_id.to_string(),
            },
        )
        .await
    }

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

    /// Move/rename a file or directory within a registered project or open
    /// kiln. All containment checks are daemon-side; overwrites are rejected.
    pub async fn fs_move(
        &self,
        root: &str,
        kind: FsRootKind,
        from_rel: &str,
        to_rel: &str,
    ) -> Result<crate::FsMoveReply> {
        self.call(
            RpcMethod::FsMove,
            FsMoveRequest {
                root: root.to_string(),
                kind,
                from_rel: from_rel.to_string(),
                to_rel: to_rel.to_string(),
            },
        )
        .await
    }

    /// Create a folder (and missing parents) inside a registered project or
    /// open kiln.
    pub async fn fs_mkdir(&self, root: &str, kind: FsRootKind, rel_path: &str) -> Result<()> {
        let _: serde_json::Value = self
            .call(
                RpcMethod::FsMkdir,
                FsPathRequest {
                    root: root.to_string(),
                    kind,
                    rel_path: rel_path.to_string(),
                },
            )
            .await?;
        Ok(())
    }

    /// Move a file/directory to the root's `.crucible/trash/`. Kiln notes are
    /// dropped from the index inline (backlinks re-resolve immediately).
    pub async fn fs_trash(
        &self,
        root: &str,
        kind: FsRootKind,
        rel_path: &str,
    ) -> Result<crate::FsTrashReply> {
        self.call(
            RpcMethod::FsTrash,
            FsPathRequest {
                root: root.to_string(),
                kind,
                rel_path: rel_path.to_string(),
            },
        )
        .await
    }

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

    pub async fn project_get(&self, path: &Path) -> Result<Option<crucible_core::Project>> {
        let result: serde_json::Value = self
            .call_with_retry(
                RpcMethod::ProjectGet,
                PathRequest {
                    path: path.to_string_lossy().to_string(),
                },
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }
}
