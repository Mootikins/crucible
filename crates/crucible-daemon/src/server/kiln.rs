use super::*;
use crucible_config::{ProjectConfig, KilnConfig, KilnMeta, read_project_config, write_project_config, read_kiln_config, write_kiln_config, DataClassification};

pub(crate) async fn handle_kiln_open(
    req: Request,
    km: &Arc<KilnManager>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let path = require_param!(req, "path", as_str);
    let kiln_path = Path::new(path);

    let process = optional_param!(req, "process", as_bool).unwrap_or(false);
    let force = optional_param!(req, "force", as_bool).unwrap_or(false);

    if let Err(e) = km.open(kiln_path).await {
        return internal_error(req.id, e);
    }

    if let Some(handle) = km.get(kiln_path).await {
        let store = handle.as_note_store();
        let loader_guard = plugin_loader.lock().await;
        if let Some(ref loader) = *loader_guard {
            if let Err(e) = loader.upgrade_with_storage(store, kiln_path) {
                warn!("Failed to upgrade Lua modules with storage: {}", e);
            }
        }
    }

    if process {
        match km.open_and_process(kiln_path, force).await {
            Ok((discovered, processed, skipped, errors)) => {
                if let Err(e) = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_complete",
                    serde_json::json!({
                        "kiln": path,
                        "discovered": discovered,
                        "processed": processed,
                        "skipped": skipped,
                        "errors": errors.len()
                    }),
                )) {
                    tracing::debug!("Failed to send process_complete event: {e}");
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

pub(crate) async fn handle_kiln_list(req: Request, km: &Arc<KilnManager>) -> Response {
    let kilns = km.list().await;
    let list: Vec<_> = kilns
        .iter()
        .map(|(path, name, last_access)| {
            serde_json::json!({
                "path": path.to_string_lossy(),
                "name": name,
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
    let path_str = require_param!(req, "path", as_str);
    let classification_str = require_param!(req, "classification", as_str);

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
                kilns: vec![crucible_config::KilnAttachment {
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
    if let Some(kiln) = config.kilns.iter_mut().next() {
        kiln.data_classification = Some(classification);
        updated = true;
    }

    if !updated {
        // No kiln entries — add one
        config.kilns.push(crucible_config::KilnAttachment {
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
    let kiln_path = require_param!(req, "kiln", as_str);
    let vector_arr = require_param!(req, "vector", as_array);
    let vector: Vec<f32> = vector_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_f64().map(|f| f as f32))
        .collect();
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    // Get or open connection to the kiln
    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    // Execute vector search using the backend-agnostic method
    match handle.search_vectors(vector, limit).await {
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

pub(crate) async fn handle_list_notes(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path_filter = optional_param!(req, "path_filter", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.list_notes(path_filter).await {
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

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.get_note_by_name(name).await {
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

    let note: NoteRecord = match serde_json::from_value(note_json.clone()) {
        Ok(n) => n,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid note record: {}", e),
            )
        }
    };

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.upsert(note).await {
        Ok(events) => Response::success(
            req.id,
            serde_json::json!({
                "status": "ok",
                "events_count": events.len()
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_note_get(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let path = require_param!(req, "path", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.get(path).await {
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

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.delete(path).await {
        Ok(_event) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_note_list(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.list().await {
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
    let kiln_path = require_param!(req, "kiln", as_str);
    let file_path = require_param!(req, "path", as_str);

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

pub(crate) async fn handle_process_batch(
    req: Request,
    km: &Arc<KilnManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let request_id = req.id.clone();
    let kiln_path = require_param!(req, "kiln", as_str);
    let paths_arr = require_param!(req, "paths", as_array);
    let paths: Vec<std::path::PathBuf> = paths_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(std::path::PathBuf::from))
        .collect();
    let batch_id = request_id
        .as_ref()
        .map(|id| match id {
            RequestId::Number(n) => format!("batch-{}", n),
            RequestId::String(s) => format!("batch-{}", s),
        })
        .unwrap_or_else(|| "batch-unknown".to_string());

    // Emit start event
    if let Err(e) = event_tx.send(SessionEventMessage::new(
        "process",
        "process_start",
        serde_json::json!({
            "type": "process_start",
            "batch_id": &batch_id,
            "total": paths.len(),
            "kiln": kiln_path
        }),
    )) {
        tracing::debug!("Failed to send process_start event: {e}");
    }

    let mut processed = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for path in &paths {
        match km.process_file(Path::new(kiln_path), path).await {
            Ok(true) => {
                processed += 1;
                if let Err(e) = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_progress",
                    serde_json::json!({
                        "type": "process_progress",
                        "batch_id": &batch_id,
                        "file": path.to_string_lossy(),
                        "result": "processed"
                    }),
                )) {
                    tracing::debug!("Failed to send process_progress event: {e}");
                }
            }
            Ok(false) => {
                skipped += 1;
                if let Err(e) = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_progress",
                    serde_json::json!({
                        "type": "process_progress",
                        "batch_id": &batch_id,
                        "file": path.to_string_lossy(),
                        "result": "skipped"
                    }),
                )) {
                    tracing::debug!("Failed to send process_progress event: {e}");
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                errors.push((path.clone(), error_msg.clone()));
                if let Err(e) = event_tx.send(SessionEventMessage::new(
                    "process",
                    "process_progress",
                    serde_json::json!({
                        "type": "process_progress",
                        "batch_id": &batch_id,
                        "file": path.to_string_lossy(),
                        "result": "error",
                        "error_msg": error_msg
                    }),
                )) {
                    tracing::debug!("Failed to send process_progress event: {e}");
                }
            }
        }
    }

    // Emit completion event
    if let Err(e) = event_tx.send(SessionEventMessage::new(
        "process",
        "process_complete",
        serde_json::json!({
            "type": "process_complete",
            "batch_id": &batch_id,
            "processed": processed,
            "skipped": skipped,
            "errors": errors.len()
        }),
    )) {
        tracing::debug!("Failed to send process_complete event: {e}");
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
