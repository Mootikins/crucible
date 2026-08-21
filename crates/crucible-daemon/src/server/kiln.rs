use super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::config::{
    read_kiln_config, read_project_config, write_kiln_config, write_project_config,
    DataClassification, KilnConfig, KilnMeta, ProjectConfig,
};
use crucible_core::storage::Scope;

/// Derive the read authority for a kiln-scoped RPC request.
///
/// Authority is always derived from the `kiln` parameter — callers cannot
/// supply a wider scope via a `scope` field in the request (the historical
/// `decode_request_scope` did, which made the storage filter enforce
/// caller-controlled input rather than a session boundary). Any `scope`
/// in `req.params` is now ignored.
///
/// Canonicalization is best-effort: if `kiln_path` doesn't yet resolve
/// (e.g. during early setup) the unchecked path is used so the request
/// still reaches the storage layer.
fn request_scope(kiln_path: &Path) -> Scope {
    Scope::workspace(kiln_path).unwrap_or_else(|_| Scope::workspace_unchecked(kiln_path))
}

pub(crate) async fn handle_kiln_open(
    req: Request,
    km: &Arc<KilnManager>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::KilnOpenRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = Path::new(&params.path);
    let process = params.process;
    let force = params.force;

    if let Err(e) = km.open(kiln_path).await {
        return internal_error(req.id, e);
    }

    if let Some(handle) = km.get(kiln_path).await {
        let store = handle.as_note_store();
        let property_store = handle.as_property_store();
        let loader_guard = plugin_loader.lock().await;
        if let Some(ref loader) = *loader_guard {
            // The name, resolved through the registry the manager holds. A
            // caller that opened an unregistered directory names no kiln to
            // the plugins, which is what `None` says.
            let kiln_name = km.kiln_name_for(kiln_path);
            if let Err(e) = loader.upgrade_with_storage(store, kiln_path, kiln_name.as_ref()) {
                warn!("Failed to upgrade Lua modules with storage: {}", e);
            }
            if let Err(e) = loader.upgrade_with_property_store(property_store) {
                warn!("Failed to upgrade Lua storage module: {}", e);
            }
        }
    }

    if process {
        match km.open_and_process(kiln_path, force).await {
            Ok((discovered, processed, skipped, errors)) => {
                if !emit_event(
                    event_tx,
                    SessionEventMessage::new(
                        "process",
                        "process_complete",
                        serde_json::json!({
                            "kiln": params.path,
                            "discovered": discovered,
                            "processed": processed,
                            "skipped": skipped,
                            "errors": errors.len()
                        }),
                    ),
                ) {
                    tracing::debug!("process_complete event had no subscribers");
                }

                Response::success(
                    req.id,
                    serde_json::json!({
                        "status": "ok",
                        "discovered": discovered,
                        "processed": processed,
                        "skipped": skipped,
                        "errors": errors.iter().map(|(p, e)| {
                            serde_json::json!({"path": p.to_string_lossy(), "error": e})
                        }).collect::<Vec<_>>()
                    }),
                )
            }
            Err(e) => {
                warn!("Processing failed for kiln {:?}: {}", kiln_path, e);
                Response::success(
                    req.id,
                    serde_json::json!({
                        "status": "ok",
                        "process_error": e.to_string()
                    }),
                )
            }
        }
    } else {
        Response::success(req.id, serde_json::json!({"status": "ok"}))
    }
}

pub(crate) async fn handle_kiln_close(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match km.close(Path::new(path)).await {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
    }
}

/// List the open kilns.
///
/// `name` is the **registry key** — the name every other API call answers to,
/// and the one the web layer joins a session's kilns against. It used to be the
/// `[kiln] name` out of the kiln's own `kiln.toml`: a name the corpus asserts
/// about itself, which two kilns can claim at once and which no caller can say
/// back to us. A directory with no registry entry falls back to the name that
/// *would* be derived from its basename, so the picker still has something to
/// show for a kiln opened by another door.
///
/// `path` stays, by design — this is the one listing whose job is to say where
/// a kiln lives.
pub(crate) async fn handle_kiln_list(
    req: Request,
    km: &Arc<KilnManager>,
    registry: &crate::kiln_registry::KilnRegistry,
    data_home: &Path,
) -> Response {
    let kilns = km.list().await;
    let list: Vec<_> = kilns
        .iter()
        // The daemon data root (~/.crucible) gets opened as the fallback kiln
        // for kiln-less sessions, but it is config/session storage — not a
        // user kiln. Listing it would surface ".crucible" in every kiln picker.
        .filter(|(path, _, _)| path != data_home)
        .map(|(path, name, last_access)| {
            let registered = registry
                .name_for(path)
                .map(ToString::to_string)
                .or_else(|| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(crucible_core::config::KilnName::normalize)
                        .map(|n| n.to_string())
                })
                .or_else(|| name.clone())
                .unwrap_or_default();
            serde_json::json!({
                "path": path.to_string_lossy(),
                "name": registered,
                "last_access_secs_ago": last_access.elapsed().as_secs()
            })
        })
        .collect();
    Response::success(req.id, list)
}

pub(crate) async fn handle_kiln_set_classification(
    req: Request,
    _km: &Arc<KilnManager>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::KilnSetClassificationRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let path_str = params.path.as_str();
    let classification_str = params.classification.as_str();

    let classification = match DataClassification::from_str_insensitive(classification_str) {
        Some(c) => c,
        None => {
            let valid: Vec<&str> = DataClassification::all()
                .iter()
                .map(|c| c.as_str())
                .collect();
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "Invalid classification '{}'. Valid values: {}",
                    classification_str,
                    valid.join(", ")
                ),
            );
        }
    };

    let workspace = Path::new(path_str);
    let crucible_dir = workspace.join(".crucible");
    if let Err(e) = std::fs::create_dir_all(&crucible_dir) {
        return internal_error(req.id, e);
    }

    // Read existing project config or create default
    let mut config = match read_project_config(workspace) {
        Some(c) => c,
        None => {
            // Create default ProjectConfig with a single kiln at "."
            ProjectConfig {
                project: None,
                kilns: vec![crucible_core::config::KilnAttachment {
                    path: ".".into(),
                    name: None,
                    data_classification: None,
                }],
                security: Default::default(),
            }
        }
    };

    // Update classification on the first kiln entry (or the matching one)
    let mut updated = false;
    if let Some(kiln) = config.kilns.first_mut() {
        kiln.data_classification = Some(classification);
        updated = true;
    }

    if !updated {
        // No kiln entries — add one
        config.kilns.push(crucible_core::config::KilnAttachment {
            path: ".".into(),
            name: None,
            data_classification: Some(classification),
        });
    }

    // Write project config
    if let Err(e) = write_project_config(workspace, &config) {
        return internal_error(req.id, e);
    }

    // Ensure kiln.toml exists with default metadata
    if read_kiln_config(workspace).is_none() {
        let kiln_name = workspace
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "kiln".to_string());
        let kiln_config = KilnConfig {
            kiln: KilnMeta { name: kiln_name },
        };
        if let Err(e) = write_kiln_config(workspace, &kiln_config) {
            return internal_error(req.id, e);
        }
    }

    info!(
        "Set data classification to '{}' for workspace at {:?}",
        classification.as_str(),
        workspace
    );

    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "classification": classification.as_str(),
            "path": path_str,
        }),
    )
}

pub(crate) async fn handle_search_vectors(req: Request, km: &Arc<KilnManager>) -> Response {
    // `params.scope` is deliberately not read: authority comes from `kiln`
    // alone (see `request_scope`). Deserializing the field rather than
    // dropping it keeps the client's struct honest about what it sends.
    let params = match typed_params::<crate::rpc_client::SearchVectorsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln.as_str();
    let vector = params.vector;
    let limit = params.limit;

    let scope = request_scope(Path::new(kiln_path));

    // Get or open connection to the kiln
    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    // Execute vector search; scope filters at the SQL layer.
    match handle.search_vectors(vector, limit, &scope).await {
        Ok(results) => {
            let json_results: Vec<_> = results
                .into_iter()
                .map(|(doc_id, score)| {
                    serde_json::json!({
                        "document_id": doc_id,
                        "score": score
                    })
                })
                .collect();
            Response::success(req.id, json_results)
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// Full-text search over note titles AND bodies (FTS5, BM25-ranked).
///
/// The `search_vectors` sibling answers "what is this about"; this one
/// answers "which note says this word", which is what `cru search` needs and
/// what listing notes and matching their filenames never could.
pub(crate) async fn handle_search_text(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let query = require_param!(req, "query", as_str);
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    // Implicit AND over words, user quotes for phrases, operators and
    // punctuation kept literal — see `build_match_query` for the contract.
    let fts_query = crate::storage::sqlite::fts::build_match_query(query);

    match handle.text.search(&fts_query, limit).await {
        Ok(results) => {
            let json_results: Vec<_> = results
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "path": r.path,
                        "title": r.title,
                        "snippet": r.snippet,
                        "rank": r.rank,
                    })
                })
                .collect();
            Response::success(req.id, json_results)
        }
        Err(e) => internal_error(req.id, anyhow::anyhow!(e)),
    }
}

pub(crate) async fn handle_embed_query(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let text = require_param!(req, "text", as_str);

    let Some(config) = km.enrichment_config().cloned() else {
        // Do not name `[embedding]` here: the config loader rejects that
        // section outright as legacy, so a user who followed this advice
        // would brick every subsequent `cru` command.
        return internal_error(
            req.id,
            anyhow::anyhow!(
                "no embedding provider configured; add one under \
                 [llm.providers.<name>] and set [llm].default, then run `cru doctor` to verify"
            ),
        );
    };

    // Ensure the kiln is open so the daemon has loaded its config + caches.
    if let Err(e) = km.get_or_open(Path::new(kiln_path)).await {
        return internal_error(req.id, e);
    }

    match crate::embedding::get_or_create_embedding_provider(&config).await {
        Ok(provider) => match provider.embed(text).await {
            Ok(vector) => Response::success(req.id, serde_json::json!({ "vector": vector })),
            Err(e) => internal_error(req.id, anyhow::anyhow!(e)),
        },
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_list_notes(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path_filter = optional_param!(req, "path_filter", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.list_notes(path_filter, &scope).await {
        Ok(notes) => {
            let json_notes: Vec<_> = notes
                .into_iter()
                .map(|n| {
                    serde_json::json!({
                        "name": n.name,
                        "path": n.path,
                        "title": n.title,
                        "tags": n.tags,
                        "updated_at": n.updated_at.map(|t| t.to_rfc3339())
                    })
                })
                .collect();
            Response::success(req.id, json_notes)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_get_note_by_name(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let name = require_param!(req, "name", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.get_note_by_name(name, &scope).await {
        Ok(Some(note)) => Response::success(
            req.id,
            serde_json::json!({
                "path": note.path,
                "title": note.title,
                "tags": note.tags,
                "links_to": note.links_to,
                "content_hash": note.content_hash.to_string()
            }),
        ),
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_get_backlinks(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let name = require_param!(req, "name", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.get_backlinks(name, &scope).await {
        Ok(Some((note, backlinks, spans))) => Response::success(
            req.id,
            serde_json::json!({
                "path": note.path,
                "title": note.title,
                "backlinks": backlinks
                    .into_iter()
                    .map(|b| {
                        let mut v = serde_json::json!({
                            "name": b.name,
                            "path": b.path,
                            "title": b.title,
                        });
                        // Byte span of the first link occurrence in the
                        // source — lets clients jump to the referencing
                        // block without re-scanning the file.
                        if let Some((start, end)) = spans.get(&b.path) {
                            v["span_start"] = serde_json::json!(start);
                            v["span_end"] = serde_json::json!(end);
                        }
                        v
                    })
                    .collect::<Vec<_>>()
            }),
        ),
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_kiln_graph(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let notes = match handle.list_notes(None, &scope).await {
        Ok(n) => n,
        Err(e) => return internal_error(req.id, e),
    };

    let edges = match handle.as_note_store().graph_links().await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    // Only surface edges whose source is a note the caller can see, and drop
    // resolved edges pointing at an out-of-scope note so `links[].target`
    // (resolved) always joins a `notes[].path`. Dangling edges keep their
    // target_key — they name no note by definition.
    let visible: std::collections::HashSet<&str> = notes.iter().map(|n| n.path.as_str()).collect();

    let notes_json: Vec<_> = notes
        .iter()
        .map(|n| {
            let title = n
                .title
                .as_deref()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    Path::new(&n.path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&n.path)
                        .to_string()
                });
            serde_json::json!({
                "path": n.path,
                "title": title,
                "tags": n.tags,
            })
        })
        .collect();

    let links_json: Vec<_> = edges
        .into_iter()
        .filter(|e| visible.contains(e.source.as_str()))
        .filter(|e| !e.resolved || visible.contains(e.target.as_str()))
        .map(|e| {
            serde_json::json!({
                "source": e.source,
                "target": e.target,
                "resolved": e.resolved,
            })
        })
        .collect();

    Response::success(
        req.id,
        serde_json::json!({
            "notes": notes_json,
            "links": links_json,
        }),
    )
}

// =============================================================================
// NoteStore RPC Handlers
// =============================================================================

pub(crate) async fn handle_note_upsert(req: Request, km: &Arc<KilnManager>) -> Response {
    use crucible_core::storage::NoteRecord;

    let kiln_path = require_param!(req, "kiln", as_str);

    let note_json = match req.params.get("note") {
        Some(n) => n,
        None => return Response::error(req.id, INVALID_PARAMS, "Missing 'note' parameter"),
    };

    let mut note: NoteRecord = match serde_json::from_value(note_json.clone()) {
        Ok(n) => n,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid note record: {}", e),
            )
        }
    };

    // Bridge authority is the workspace scope of the kiln being written to;
    // a note declared in this RPC cannot exceed it. This closes the gap left
    // when the DaemonVaultBridge write-validation was removed by the
    // "cascade-orphaned APIs" cleanup — without this check, a Lua plugin in
    // kiln A could write a note with `properties.scope` pointing at kiln B
    // and have it become visible to B's read authority.
    let bridge_authority = request_scope(Path::new(kiln_path));

    let declared_scope = match note.properties.get("scope") {
        Some(v) => match Scope::from_property_value(v) {
            Some(Ok(s)) => s.bind_to_workspace(bridge_authority.path()),
            Some(Err(e)) => {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!("unsupported scope in note properties: {}", e),
                );
            }
            None => bridge_authority.clone(),
        },
        None => bridge_authority.clone(),
    };

    if !declared_scope.same_workspace(&bridge_authority) {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "declared scope {} exceeds session write authority {}",
                declared_scope, bridge_authority
            ),
        );
    }

    // Stamp the resolved scope onto the record so unscoped notes inherit
    // the bridge authority — closes the legacy-unstamped escape hatch
    // (M6) at the RPC boundary.
    note.properties
        .insert("scope".to_string(), declared_scope.to_property_value());

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.upsert(note).await {
        Ok(events) => {
            // Announce, do not just count. This handler writes through
            // `NoteStore` directly rather than through the pipeline, so it is
            // the only thing holding these events — reporting `events_count`
            // and dropping them is how an RPC-written note fired no
            // `note:created` while a watcher-written one did.
            km.announce(&events);
            Response::success(
                req.id,
                serde_json::json!({
                    "status": "ok",
                    "events_count": events.len()
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_note_get(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path = require_param!(req, "path", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.get(path, &scope).await {
        Ok(Some(note)) => match serde_json::to_value(&note) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_note_delete(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path = require_param!(req, "path", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();

    // Enforce the same authority boundary reads use: only delete a note the
    // request scope can actually see. Otherwise a narrower authority could
    // delete notes it isn't allowed to read.
    match note_store.get(path, &scope).await {
        Ok(Some(_)) => {}
        Ok(None) => return Response::success(req.id, serde_json::json!({"status": "not_found"})),
        Err(e) => return internal_error(req.id, e),
    }

    match note_store.delete(path).await {
        // The embedding lives on the deleted `notes` row, so there is no
        // separate vector index to clean up.
        Ok(event) => {
            // Same reason as `handle_note_upsert`: this path holds the only
            // copy of the event, so binding it to `_` dropped it.
            km.announce(std::slice::from_ref(&event));
            Response::success(req.id, serde_json::json!({"status": "ok"}))
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_note_list(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.list(&scope).await {
        Ok(notes) => match serde_json::to_value(&notes) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e) => internal_error(req.id, e),
    }
}

// =============================================================================
// Pipeline RPC Handlers
// =============================================================================

pub(crate) async fn handle_process_file(req: Request, km: &Arc<KilnManager>) -> Response {
    let params = match typed_params::<crate::rpc_client::ProcessFileRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = params.kiln.as_str();
    let file_path = params.path.as_str();

    match km
        .process_file(Path::new(kiln_path), Path::new(file_path))
        .await
    {
        Ok(processed) => Response::success(
            req.id,
            serde_json::json!({
                "status": if processed { "processed" } else { "skipped" },
                "path": file_path
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

/// Process an explicit list of paths, returning the counts the caller prints.
///
/// **Emits nothing, deliberately.** This handler used to broadcast a
/// `process_start`, a `process_progress` per file and a batch
/// `process_complete`, all addressed to the synthetic session id `"process"`,
/// which neither delivery filter passes (`chat_runner/stream.rs`, the web
/// `EventBroker`). Both callers are `cru process` — a single file, or watch
/// mode's changed set — and both print from this response, so the events had no
/// reader and only restated the return value.
///
/// The deleted events were on the *fast* path. Full-kiln indexing, the slow one,
/// goes through `kiln.open { process: true }` →
/// `KilnManager::open_and_process` → `KilnManager::process_batch`, whose per-file
/// loop has never emitted anything; a real progress producer belongs there,
/// throttled and addressed to `WILDCARD_SESSION` so both surfaces can receive
/// it — not here.
pub(crate) async fn handle_process_batch(req: Request, km: &Arc<KilnManager>) -> Response {
    let request_id = req.id.clone();
    let kiln_path = require_param!(req, "kiln", as_str);
    let paths_arr = require_param!(req, "paths", as_array);
    let paths: Vec<std::path::PathBuf> = paths_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(std::path::PathBuf::from))
        .collect();

    let mut processed = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for path in &paths {
        match km.process_file(Path::new(kiln_path), path).await {
            Ok(true) => processed += 1,
            Ok(false) => skipped += 1,
            Err(e) => errors.push((path.clone(), e.to_string())),
        }
    }

    Response::success(
        request_id,
        serde_json::json!({
            "processed": processed,
            "skipped": skipped,
            "errors": errors
                .iter()
                .map(|(p, err)| {
                    serde_json::json!({
                        "path": p.to_string_lossy(),
                        "error": err
                    })
                })
                .collect::<Vec<_>>()
        }),
    )
}

pub(crate) async fn handle_suggest_links(req: Request, km: &Arc<KilnManager>) -> Response {
    let text = require_param!(req, "text", as_str);
    let kiln_path = require_param!(req, "kiln", as_str);

    let scope = request_scope(Path::new(kiln_path));

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let notes = match handle.list_notes(None, &scope).await {
        Ok(n) => n,
        Err(e) => return internal_error(req.id, e),
    };

    let note_names: Vec<String> = notes.into_iter().map(|n| n.name).collect();
    let suggestions = crate::tools::autolink::suggest_links(text, &note_names);

    Response::success(req.id, serde_json::json!({ "suggestions": suggestions }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn list_request() -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "kiln.list",
            "params": {},
        }))
        .unwrap()
    }

    /// `kiln.list`'s `name` is the registry key, not the name the kiln asserts
    /// about itself in its own `kiln.toml`.
    ///
    /// The web layer joins a session's `kilns` against this listing, and a
    /// session's kilns are registry names — so a self-asserted name matches
    /// nothing, and two kilns are free to claim the same one. The fixture puts
    /// a `kiln.toml` in the directory precisely so the wrong answer is
    /// available to be returned.
    #[tokio::test]
    async fn kiln_list_reports_the_registry_name_not_the_kilns_self_description() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let kiln_dir = tmp.path().join("on-disk-directory");
        std::fs::create_dir_all(kiln_dir.join(".crucible")).unwrap();
        std::fs::write(
            kiln_dir.join(".crucible").join("kiln.toml"),
            "[kiln]\nname = \"Self Asserted\"\n",
        )
        .unwrap();

        let km = Arc::new(KilnManager::new());
        km.open(&kiln_dir).await.expect("open the kiln");
        let registry = crate::test_support::kiln_registry(&data_home, &[("work", &kiln_dir)]);

        let resp = handle_kiln_list(list_request(), &km, &registry, &data_home).await;

        let listed = resp.result.expect("kiln.list returns a list");
        let entry = &listed.as_array().expect("an array")[0];
        assert_eq!(
            entry["name"], "work",
            "the registry key is the name every other call answers to: {listed}"
        );
        assert_ne!(
            entry["name"], "Self Asserted",
            "a kiln's own idea of its name must not be what the API reports"
        );
        assert_eq!(
            entry["path"],
            kiln_dir.to_string_lossy().as_ref(),
            "the path stays — this is the one listing whose job is to say where a kiln lives"
        );
    }

    /// A directory open under some other door — `kiln.open`, which has no
    /// registration floor — has no registry entry at all. It still needs a
    /// name to show in a picker, and the fallback is the one that *would* be
    /// derived from its basename rather than the kiln's self-description.
    #[tokio::test]
    async fn an_unregistered_open_kiln_falls_back_to_its_derived_name() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let kiln_dir = tmp.path().join("My Vault");
        std::fs::create_dir_all(&kiln_dir).unwrap();

        let km = Arc::new(KilnManager::new());
        km.open(&kiln_dir).await.expect("open the kiln");
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        assert!(
            registry.name_for(&kiln_dir).is_none(),
            "precondition: the directory must be unregistered"
        );

        let resp = handle_kiln_list(list_request(), &km, &registry, &data_home).await;

        let listed = resp.result.expect("kiln.list returns a list");
        assert_eq!(listed.as_array().expect("an array")[0]["name"], "my-vault");
    }
}
