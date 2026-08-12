//! Storage and Kiln RPC methods
//!
//! Methods for managing kilns, notes, and storage operations.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::warn;

use super::types::{EmptyParams, KilnPathRequest, PathRequest};
use super::DaemonClient;

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

/// Request for `search_vectors`.
///
/// `scope` is the request authority — defaults server-side to
/// `Scope::Workspace { path: kiln }` when absent. Hits whose stored
/// `properties.scope` is outside the authority are filtered out (via a
/// SQLite post-filter on the Lance hit IDs).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchVectorsRequest {
    pub kiln: String,
    pub vector: Vec<f32>,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<crucible_core::storage::Scope>,
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
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrepSearchRequest {
    pub root: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    pub limit: usize,
    pub case_insensitive: bool,
}

/// Request for `fs.list_dir`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FsListDirRequest {
    pub root: String,
    pub rel_path: String,
    pub show_ignored: bool,
    pub show_hidden: bool,
}

/// Request for `fs.move`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FsMoveRequest {
    pub root: String,
    /// `"project"` or `"kiln"` — selects the daemon-side allowlist.
    pub kind: String,
    pub from_rel: String,
    pub to_rel: String,
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

impl DaemonClient {
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
        self.typed_call(
            "kiln.open",
            KilnOpenRequest {
                path: path.to_string_lossy().to_string(),
                process,
                force,
            },
        )
        .await
    }

    pub async fn kiln_set_classification(&self, path: &Path, classification: &str) -> Result<()> {
        let _: serde_json::Value = self
            .typed_call(
                "kiln.set_classification",
                KilnSetClassificationRequest {
                    path: path.to_string_lossy().to_string(),
                    classification: classification.to_string(),
                },
            )
            .await?;
        Ok(())
    }

    pub async fn kiln_list(&self) -> Result<Vec<serde_json::Value>> {
        let result: serde_json::Value = self.typed_call("kiln.list", EmptyParams {}).await?;
        Ok(result.as_array().cloned().unwrap_or_default())
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
            .typed_call(
                "embed.query",
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
    ) -> Result<Vec<TextSearchHit>> {
        let result: serde_json::Value = self
            .typed_call(
                "search_text",
                SearchTextRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    query: query.to_string(),
                    limit,
                },
            )
            .await?;

        Ok(result
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| {
                Some(TextSearchHit {
                    path: item.get("path")?.as_str()?.to_string(),
                    title: item.get("title")?.as_str()?.to_string(),
                    snippet: item
                        .get("snippet")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    rank: item.get("rank").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
            })
            .collect())
    }

    /// Semantic vector search.
    ///
    /// Passes the caller's authority to the daemon so cross-scope hits are
    /// filtered out before they cross the RPC boundary. `scope = None`
    /// means "server default" (workspace scope derived from kiln).
    pub async fn search_vectors(
        &self,
        kiln_path: &Path,
        vector: &[f32],
        limit: usize,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Vec<(String, f64)>> {
        let result: serde_json::Value = self
            .typed_call(
                "search_vectors",
                SearchVectorsRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    vector: vector.to_vec(),
                    limit,
                    scope,
                },
            )
            .await?;

        let results: Vec<(String, f64)> = result
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| {
                let doc_id = item.get("document_id")?.as_str()?.to_string();
                let score = item.get("score")?.as_f64()?;
                Some((doc_id, score))
            })
            .collect();

        Ok(results)
    }

    /// Ripgrep-style content search over `root` (which must be inside a
    /// registered project or open kiln — the daemon enforces containment).
    /// Returns the hits plus whether they were capped at `limit`.
    pub async fn search_grep(
        &self,
        root: &str,
        query: &str,
        glob: Option<&str>,
        limit: usize,
        case_insensitive: bool,
    ) -> Result<crate::GrepSearchResponse> {
        self.typed_call(
            "search_grep",
            GrepSearchRequest {
                root: root.to_string(),
                query: query.to_string(),
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
    ) -> Result<Vec<(String, String, Option<String>, Vec<String>, Option<String>)>> {
        let result: serde_json::Value = self
            .typed_call(
                "list_notes",
                ListNotesRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    path_filter: path_filter.map(|f| f.to_string()),
                    scope,
                },
            )
            .await?;

        let notes: Vec<_> = result
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let name = item.get("name").and_then(|v| v.as_str());
                let path = item.get("path").and_then(|v| v.as_str());

                if name.is_none() || path.is_none() {
                    warn!(
                        idx,
                        has_name = name.is_some(),
                        has_path = path.is_some(),
                        "Skipping malformed note record in list_notes response"
                    );
                    return None;
                }

                let name = name.unwrap().to_string();
                let path = path.unwrap().to_string();
                let title = item.get("title").and_then(|v| v.as_str()).map(String::from);
                let tags: Vec<String> = item
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let updated_at = item
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                Some((name, path, title, tags, updated_at))
            })
            .collect();

        Ok(notes)
    }

    /// Case-insensitive fuzzy lookup by path or title. `scope = None`
    /// defaults to the kiln's workspace authority server-side.
    pub async fn get_note_by_name(
        &self,
        kiln_path: &Path,
        name: &str,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Option<serde_json::Value>> {
        let result: serde_json::Value = self
            .typed_call(
                "get_note_by_name",
                GetNoteByNameRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    name: name.to_string(),
                    scope,
                },
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    /// Resolve a note by name and return the notes that wikilink to it.
    ///
    /// Returns `None` when the name resolves to no note. On success the
    /// value is `{ path, title, backlinks: [{ name, path, title }] }`.
    pub async fn get_backlinks(
        &self,
        kiln_path: &Path,
        name: &str,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<Option<serde_json::Value>> {
        let result: serde_json::Value = self
            .typed_call(
                "get_backlinks",
                GetBacklinksRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    name: name.to_string(),
                    scope,
                },
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    /// The full note-link graph for a kiln. `scope = None` defaults to the
    /// kiln's workspace authority server-side. Returns the raw
    /// `{ notes: [...], links: [...] }` value verbatim.
    pub async fn kiln_graph(
        &self,
        kiln_path: &Path,
        scope: Option<crucible_core::storage::Scope>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "kiln.graph",
            KilnGraphRequest {
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
    ) -> Result<Vec<serde_json::Value>> {
        let result: serde_json::Value = self
            .typed_call(
                "suggest_links",
                SuggestLinksRequest {
                    kiln: kiln_path.to_string_lossy().to_string(),
                    text: text.to_string(),
                    scope,
                },
            )
            .await?;

        Ok(result
            .get("suggestions")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default())
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
            .typed_call(
                "note.upsert",
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
            .typed_call(
                "note.get",
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
            .typed_call(
                "note.delete",
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
        self.typed_call(
            "note.list",
            NoteListRequest {
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
            .typed_call(
                "process_batch",
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
        self.typed_call(
            "storage.verify",
            KilnPathRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn storage_cleanup(&self, kiln_path: &Path) -> Result<serde_json::Value> {
        self.typed_call(
            "storage.cleanup",
            KilnPathRequest {
                kiln: kiln_path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn storage_backup(&self, kiln_path: &Path, dest: &Path) -> Result<serde_json::Value> {
        self.typed_call(
            "storage.backup",
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
        self.typed_call(
            "storage.restore",
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
        self.typed_call(
            "mcp.start",
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
        self.typed_call("mcp.stop", EmptyParams {}).await
    }

    /// Get the status of the daemon-managed MCP server.
    pub async fn mcp_status(&self) -> Result<serde_json::Value> {
        self.typed_call("mcp.status", EmptyParams {}).await
    }

    // =========================================================================
    // Project RPC Methods
    // =========================================================================

    pub async fn project_register(&self, path: &Path) -> Result<crucible_core::Project> {
        self.typed_call_with_retry(
            "project.register",
            PathRequest {
                path: path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    pub async fn project_unregister(&self, path: &Path) -> Result<()> {
        let _: serde_json::Value = self
            .typed_call_with_retry(
                "project.unregister",
                PathRequest {
                    path: path.to_string_lossy().to_string(),
                },
            )
            .await?;
        Ok(())
    }

    pub async fn project_list(&self) -> Result<Vec<crucible_core::Project>> {
        self.typed_call_with_retry("project.list", EmptyParams {})
            .await
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
    ) -> Result<serde_json::Value> {
        let v: serde_json::Value = self
            .typed_call(
                "fs.list_dir",
                FsListDirRequest {
                    root: root.to_string(),
                    rel_path: rel_path.to_string(),
                    show_ignored,
                    show_hidden,
                },
            )
            .await?;
        Ok(v)
    }

    /// Move/rename a file or directory within a registered project or open
    /// kiln. All containment checks are daemon-side; overwrites are rejected.
    pub async fn fs_move(
        &self,
        root: &str,
        kind: &str,
        from_rel: &str,
        to_rel: &str,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "fs.move",
            FsMoveRequest {
                root: root.to_string(),
                kind: kind.to_string(),
                from_rel: from_rel.to_string(),
                to_rel: to_rel.to_string(),
            },
        )
        .await
    }

    /// Create a folder (and missing parents) inside a registered project or
    /// open kiln.
    pub async fn fs_mkdir(&self, root: &str, kind: &str, rel_path: &str) -> Result<()> {
        let _: serde_json::Value = self
            .typed_call(
                "fs.mkdir",
                serde_json::json!({ "root": root, "kind": kind, "rel_path": rel_path }),
            )
            .await?;
        Ok(())
    }

    /// Move a file/directory to the root's `.crucible/trash/`. Kiln notes are
    /// dropped from the index inline (backlinks re-resolve immediately).
    pub async fn fs_trash(
        &self,
        root: &str,
        kind: &str,
        rel_path: &str,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "fs.trash",
            serde_json::json!({ "root": root, "kind": kind, "rel_path": rel_path }),
        )
        .await
    }

    /// Rename/move a NOTE within an open kiln, rewriting unambiguous inbound
    /// wikilinks (daemon `note.move`). Returns the outcome object
    /// (`rewritten_sources`, `skipped`) for caller UX.
    pub async fn note_move(
        &self,
        kiln: &str,
        from_rel: &str,
        to_rel: &str,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "note.move",
            serde_json::json!({ "kiln": kiln, "from_rel": from_rel, "to_rel": to_rel }),
        )
        .await
    }

    /// Clone a remote git repo and register it as a project.
    ///
    /// `dest` (absolute, must not exist) overrides the configured
    /// `projects_dir/<repo-name>` default; `name` overrides the repo-name
    /// derived from the URL. Uses a generous 10-minute timeout — cloning a
    /// large repository can far exceed the default request timeout.
    pub async fn scm_clone(
        &self,
        url: &str,
        dest: Option<&Path>,
        name: Option<&str>,
    ) -> Result<crate::scm::ScmCloneResponse> {
        self.typed_call_with_timeout(
            "scm.clone",
            serde_json::json!({
                "url": url,
                "dest": dest.map(|p| p.to_string_lossy()),
                "name": name,
            }),
            std::time::Duration::from_secs(600),
        )
        .await
    }

    pub async fn project_get(&self, path: &Path) -> Result<Option<crucible_core::Project>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                "project.get",
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
