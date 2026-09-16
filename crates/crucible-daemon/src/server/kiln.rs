use super::*;
/// Derive the read authority for a kiln-scoped RPC request.
///
/// Authority is always derived from the `kiln` parameter — callers cannot
/// supply a wider scope via a `scope` field in the request (the historical
/// `decode_request_scope` did, which made the storage filter enforce
/// caller-controlled input rather than a session boundary). Any `scope`
/// in `req.params` is now ignored.
use crate::kiln_manager::request_scope;
use crate::rpc_helpers::typed_params;
use crucible_core::storage::Scope;

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

/// List the kilns a client may address.
///
/// `name` is the **registry key** — the name every other API call answers to,
/// and the one the web layer joins a session's kilns against. It used to be the
/// `[kiln] name` out of the kiln's own `kiln.toml`: a name the corpus asserts
/// about itself, which two kilns can claim at once and which no caller can say
/// back to us.
///
/// # Registered, not merely open
///
/// This listed what the *manager* held open, and a fresh daemon holds nothing.
/// Every kiln-addressed route gates on it — the web file editor checks a path
/// against this listing before it reads a byte — so a restart turned every
/// registered kiln into a 404, and a lazy kiln was unreachable for the life of
/// the process. "Which directories are kilns" is the REGISTRY's question, and
/// asking the manager was asking the wrong owner.
///
/// So the listing is the registry's entries, plus any directory the manager
/// has open that no entry names. `open` says which of the two it is, honestly,
/// and a closed row is not a dead one: the first request that addresses a kiln
/// opens it (`KilnManager::get_or_open`, used by every storage handler here).
/// `lazy` therefore keeps meaning "not opened UNASKED" rather than "never".
///
/// # What it still refuses to publish
///
/// A name the attach cannot resolve. An open directory with no registry entry
/// carries `registered: false` and no name — a client must not offer it — and
/// a registration whose directory is gone is left out entirely, because
/// attaching it would fail to open. `cru kiln list` (`kiln.registry_list`) is
/// the surface that reports a missing registration, marked `(missing)`.
///
/// `path` stays, by design — this is the one listing whose job is to say where
/// a kiln lives.
pub(crate) async fn handle_kiln_list(
    req: Request,
    km: &Arc<KilnManager>,
    registry: &crate::kiln_registry::KilnRegistry,
    data_home: &Path,
) -> Response {
    use std::collections::{HashMap, HashSet};

    // What the manager holds open. Keyed by the path it opened, which a
    // registry entry matches by either of its two spellings.
    let open: HashMap<std::path::PathBuf, std::time::Instant> = km
        .list()
        .await
        .into_iter()
        .map(|(path, _self_asserted, last_access)| (path, last_access))
        .collect();

    let mut rows = Vec::new();
    let mut claimed: HashSet<std::path::PathBuf> = HashSet::new();

    for kiln in registry.entries() {
        let opened = open
            .get(kiln.path())
            .or_else(|| open.get(kiln.resolved_path()));
        // A registration pointing at a directory that is gone is not offered:
        // the attach would fail to open it, and this listing publishes only
        // names the attach resolves.
        if opened.is_none() && !kiln.path().is_dir() {
            continue;
        }
        claimed.insert(kiln.path().to_path_buf());
        claimed.insert(kiln.resolved_path().to_path_buf());
        rows.push(serde_json::json!({
            "path": kiln.path().to_string_lossy(),
            "name": kiln.name().as_str(),
            "registered": true,
            "open": opened.is_some(),
            "last_access_secs_ago": opened.map(|at| at.elapsed().as_secs()),
        }));
    }

    for (path, last_access) in open {
        // The daemon data root gets opened as the fallback kiln for kiln-less
        // sessions, but it is config/session storage — not a user kiln.
        // Listing it would surface ".crucible" in every kiln picker.
        if path == data_home || claimed.contains(&path) || registry.name_for(&path).is_some() {
            continue;
        }
        rows.push(serde_json::json!({
            "path": path.to_string_lossy(),
            "name": "",
            "registered": false,
            "open": true,
            "last_access_secs_ago": last_access.elapsed().as_secs(),
        }));
    }

    Response::success(req.id, rows)
}

/// `kiln.register`: give a directory a name, and write it down.
///
/// Three layers, in this order, and the order is the point.
///
/// 1. **The floor.** The path is absolutized and refused by the registry
///    before anything is written — a refused path yields no entry and no name.
/// 2. **The config layer.** A name the user's config already points somewhere
///    else is refused. A registration that lands shadowed is a lie shaped like
///    success: it would sit in `kilns.json` forever, out-ranked, doing nothing.
/// 3. **The state layer, then memory.** `kilns.json` is written first, under
///    its own lock, with its own never-re-point check; the live registry takes
///    the name only after the write succeeded. The other order would serve a
///    name this daemon forgets at the next boot.
///
/// The registration is **additive at runtime**: the name resolves in this same
/// daemon process, with no restart. Adding a name re-points nothing, so it is
/// safe under the boot freeze's own reasoning — re-pointing and removal are
/// what the freeze protects, and both still wait for the next boot.
pub(crate) async fn handle_kiln_register(
    req: Request,
    registry: &Arc<crate::kiln_registry::KilnRegistry>,
    state: &Arc<crate::kiln_state::KilnStateStore>,
    config_path: Option<&Path>,
) -> Response {
    use crucible_core::config::{KilnName, RegistrationOrigin};

    let params = match typed_params::<crate::rpc_client::KilnRegisterRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    // The floor, and the one spelling both layers must agree on. Runs before
    // the name is settled, because deriving a name for a path the floor
    // refuses would mint a name for a directory that never gets an entry.
    let path = match registry.canonical_for_registration(Path::new(&params.path)) {
        Ok(path) => path,
        Err(refusal) => return Response::error(req.id, INVALID_PARAMS, refusal.to_string()),
    };

    // A caller that named one gets that name, or the rule if it is not a name.
    // A caller that named none is asking this registry to derive one, which is
    // the only place the derivation can be correct: it depends on what is
    // already registered here.
    let name = match params.name.as_deref() {
        Some(raw) => match KilnName::parse(raw) {
            Ok(name) => name,
            Err(e) => {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!(
                        "{e}. A kiln name holds `[A-Za-z0-9._- ]`, at most {} characters, \
                         does not start with a dot, and is not padded with spaces. Two names \
                         that differ only in case are one kiln.",
                        KilnName::MAX_LEN
                    ),
                )
            }
        },
        None => match registry.derive_name_for(&path) {
            Ok(name) => name,
            Err(refusal) => return Response::error(req.id, INVALID_PARAMS, refusal.to_string()),
        },
    };

    // An entry pointing at nothing is a name that resolves to nothing, and
    // every consumer that reads absence as "unconstrained" is waiting for one.
    if !path.is_dir() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "Refusing to register '{name}': '{}' is not a directory.",
                path.display()
            ),
        );
    }

    // The config layer out-ranks the state layer, so a name it claims for a
    // different directory can never be served from state.
    if let Some(existing) = registry.resolve(&name).registered() {
        if existing.origin() == RegistrationOrigin::Config && existing.path() != path {
            let file = config_path
                .map(|p| format!(" in {}", p.display()))
                .unwrap_or_default();
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "the kiln name '{name}' is declared as '{}'{file}. The config out-ranks a \
                     registration, so this one would never be used. Choose another name, or \
                     change that entry.",
                    existing.path().display()
                ),
            );
        }
    }

    let outcome = match state.register(&name, &path, params.auto, params.make_default) {
        Ok(outcome) => outcome,
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e.to_string()),
    };

    // The second layer, exactly as the file check and the registry check pair
    // up: the file outlives the process, the registry answers this one.
    if let Err(refusal) = registry.register_named(name.clone(), &path) {
        return Response::error(req.id, INVALID_PARAMS, refusal.to_string());
    }

    info!(kiln = %name, path = %path.display(), "Kiln registered");
    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "name": name.to_string(),
            "path": path.to_string_lossy(),
            "outcome": match outcome {
                crate::kiln_state::RegisterOutcome::Added => "added",
                crate::kiln_state::RegisterOutcome::AlreadyPresent => "already_present",
            },
            "state_file": state.path().to_string_lossy(),
        }),
    )
}

/// `kiln.registry_list`: every kiln name Crucible knows, and which layer owns it.
///
/// The honesty mechanism for the split ownership model. Two layers hold kiln
/// names — the config the user authored, and the state the daemon was told —
/// and a user can only act on a conflict they can see. Six situations, all
/// reported here:
///
/// | Situation | `origin` | markers |
/// |---|---|---|
/// | Declared in the config only | `config` | — |
/// | Registered in state only | `registered` | — |
/// | Both, same path | `config` | `also_registered` |
/// | Both, different paths | `config` | `shadows` names the state path |
/// | Opened by path, named by this daemon | `discovered` | — |
/// | Registered, directory gone | `registered` | `missing` |
///
/// `discovered` is a name, not a registration. Opening a directory names it in
/// the live registry (`KilnManager::open`), so a session CAN attach it and
/// `kiln.list` may publish it; nothing is written down, so the name lasts only
/// while this daemon runs. Attaching it is what records it. The row says
/// `discovered` so the user can see which names are in that state.
pub(crate) async fn handle_kiln_registry_list(
    req: Request,
    registry: &Arc<crate::kiln_registry::KilnRegistry>,
    state: &Arc<crate::kiln_state::KilnStateStore>,
    km: &Arc<KilnManager>,
    config_default_kiln: Option<&str>,
    data_home: &Path,
) -> Response {
    use crucible_core::config::RegistrationOrigin;

    let recorded = state.read().unwrap_or_default();
    // The config layer wins on `default_kiln` too, exactly as it wins on a
    // name: one rule, stated once, applied everywhere.
    let default = config_default_kiln
        .map(str::to_string)
        .or_else(|| recorded.default_kiln.clone());

    let mut rows = Vec::new();
    for kiln in registry.entries() {
        let name = kiln.name().to_string();
        // Folded, because a name resolves case-insensitively: a state entry
        // spelled `docs` under a config entry spelled `Docs` is the SAME
        // contested name, and a raw lookup would report neither the agreement
        // nor the conflict.
        let also =
            crucible_core::config::find_kiln_entry(&recorded.kilns, &name).map(|(_, entry)| entry);
        rows.push(serde_json::json!({
            "name": name,
            "path": kiln.path().to_string_lossy(),
            "origin": kiln.origin().as_str(),
            "default": default.as_deref().is_some_and(|d| {
                crucible_core::config::KilnName::fold_str(d)
                    == crucible_core::config::KilnName::fold_str(&name)
            }),
            "missing": !kiln.path().is_dir(),
            "lazy": kiln.lazy(),
            // Both layers hold the name and agree: one registration written
            // down twice, not a conflict.
            "also_registered": kiln.origin() == RegistrationOrigin::Config
                && also.is_some_and(|entry| entry.path == kiln.path()),
            // Both layers hold the name and disagree: the config wins, and the
            // state entry sits there doing nothing until `cru kiln forget`.
            "shadows": match (kiln.origin(), also) {
                (RegistrationOrigin::Config, Some(entry)) if entry.path != kiln.path() => {
                    serde_json::Value::String(entry.path.to_string_lossy().into_owned())
                }
                _ => serde_json::Value::Null,
            },
        }));
    }

    // Directories that are open but claim no name. The daemon data root is
    // opened as the fallback kiln for kiln-less sessions and is not a user
    // kiln, so it is filtered here for the same reason `kiln.list` filters it.
    for (path, _, _) in km.list().await {
        if path == data_home || registry.name_for(&path).is_some() {
            continue;
        }
        let derived = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(crucible_core::config::KilnName::normalize)
            .map(|n| n.to_string())
            .unwrap_or_default();
        rows.push(serde_json::json!({
            "name": derived,
            "path": path.to_string_lossy(),
            "origin": RegistrationOrigin::Discovered.as_str(),
            "default": false,
            "missing": !path.is_dir(),
            "lazy": false,
            "also_registered": false,
            "shadows": serde_json::Value::Null,
        }));
    }

    Response::success(
        req.id,
        serde_json::json!({
            "kilns": rows,
            "state_file": state.path().to_string_lossy(),
        }),
    )
}

/// `kiln.forget`: remove one registration from the state store.
///
/// Absence is not intent, so no config edit removes a registration. This
/// command is the only removal, which is the price of a config language with
/// conditionals: the daemon cannot tell "the user deleted the line" from "the
/// branch did not run".
///
/// A name the config declares is refused, and the refusal names the file: there
/// is nothing in the state store to forget, and the fix is an edit the user
/// makes. A name BOTH layers hold is forgotten — that is the shadowed entry,
/// and forgetting it clears the conflict.
///
/// The removal takes effect at the next daemon start. Removing a name changes
/// what an already-persisted session reference means, which is the hazard the
/// boot freeze exists for; adding one re-points nothing, which is why adding is
/// immediate and removing is not.
pub(crate) async fn handle_kiln_forget(
    req: Request,
    registry: &Arc<crate::kiln_registry::KilnRegistry>,
    state: &Arc<crate::kiln_state::KilnStateStore>,
    config_path: Option<&Path>,
) -> Response {
    use crucible_core::config::{KilnName, RegistrationOrigin};

    let params = match typed_params::<crate::rpc_client::NameRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    let held = state
        .read()
        .map(|file| crucible_core::config::find_kiln_entry(&file.kilns, &params.name).is_some())
        .unwrap_or(false);

    if !held {
        // Nothing in state. Say which of the two reasons it is, because the
        // remedies are different: edit a file, or check the name.
        let declared = KilnName::parse(&params.name).ok().and_then(|name| {
            registry
                .resolve(&name)
                .registered()
                .filter(|kiln| kiln.origin() == RegistrationOrigin::Config)
        });
        if let Some(kiln) = declared {
            // A Lua-declared entry carries its call site in the boot
            // provenance: name `file:line`, not just a file, so the user
            // can go to the exact declaration. A TOML seed names its file;
            // no provenance falls back to the config path the daemon was
            // handed.
            let declared_at = crucible_lua::get_app_config_provenance().and_then(|provenance| {
                let exact = format!("kilns.{}", params.name);
                let prefix = format!("{exact}.");
                provenance
                    .iter()
                    .find(|(path, _)| **path == exact || path.starts_with(&prefix))
                    .map(|(_, tag)| tag.detail())
            });
            let file = declared_at.unwrap_or_else(|| {
                config_path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "your config".to_string())
            });
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "the kiln '{}' is declared in {file}, at '{}'. There is nothing in {} to \
                     forget — remove the entry from {file} instead.",
                    params.name,
                    kiln.path().display(),
                    state.path().display()
                ),
            );
        }
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "no kiln named '{}' is registered in {}.",
                params.name,
                state.path().display()
            ),
        );
    }

    match state.forget(&params.name) {
        Ok(_) => {
            info!(kiln = %params.name, "Kiln registration forgotten");
            Response::success(
                req.id,
                serde_json::json!({
                    "status": "ok",
                    "name": params.name,
                    "state_file": state.path().to_string_lossy(),
                    // Said in the reply rather than only in the CLI, because
                    // every client needs to know the running daemon still
                    // answers to the name.
                    "takes_effect": "next daemon start",
                }),
            )
        }
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

pub(crate) async fn handle_search_vectors(
    req: Request,
    km: &Arc<KilnManager>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    // `params.scope` is deliberately not read: authority comes from `kiln`
    // alone, because the repository the handle opens is bound to the kiln
    // path and scopes every note read to it. Deserializing the field rather
    // than dropping it keeps the client's struct honest about what it sends.
    let params = match typed_params::<crate::rpc_client::SearchVectorsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let kiln_path = std::path::PathBuf::from(params.kiln);
    let vector = params.vector;
    let limit = params.limit;

    let handle = match km.get_or_open(&kiln_path).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    // One search path. `cru search` and `cru eval precognition` read this
    // reply, so it comes from the same block-first search the search tool
    // and precognition use. The repository scopes note reads to the kiln
    // itself. The name comes from the registry, never from the directory:
    // a `search:rerank` handler reads a hit's neighbours by kiln name, and
    // an unregistered directory stays nameless.
    let source = crate::multi_kiln_search::KilnSearchSource {
        knowledge_repo: handle.as_knowledge_repository(),
        kiln_name: km.kiln_name_for(&kiln_path),
        kiln_path,
    };
    // `search:rerank` reaches the handler VM's registry.
    let rerank = {
        let guard = plugin_loader.lock().await;
        guard.as_ref().map(|loader| {
            crate::multi_kiln_search::RerankStage::new(
                None,
                vec![(
                    (*loader.plugin_handlers()).clone(),
                    (*loader.plugin_lua()).clone(),
                )],
            )
        })
    };
    match crate::multi_kiln_search::search_across_kilns_with_stage(
        &[source],
        vector,
        limit,
        None,
        None,
        rerank.as_ref(),
    )
    .await
    {
        Ok(results) => {
            let hits: Vec<crate::rpc_client::VectorHit> = results
                .into_iter()
                .map(|hit| crate::rpc_client::VectorHit {
                    document_id: hit.document_id.0,
                    score: hit.score,
                    block: hit.block,
                    snippet: hit.snippet,
                })
                .collect();
            match serde_json::to_value(hits) {
                Ok(v) => Response::success(req.id, v),
                Err(e) => internal_error(req.id, anyhow::anyhow!(e)),
            }
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
        Ok(results) => match serde_json::to_value(results) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, anyhow::anyhow!(e)),
        },
        Err(e) => internal_error(req.id, anyhow::anyhow!(e)),
    }
}

pub(crate) async fn handle_embed_query(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_param!(req, "kiln", as_str);
    let text = require_param!(req, "text", as_str);

    // Open the kiln first, so an unknown path is refused before any embed.
    if let Err(e) = km.get_or_open(Path::new(kiln_path)).await {
        return internal_error(req.id, e);
    }

    match km.embedding_provider().await {
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
                        "updated_at": n.updated_at.map(|t| t.to_rfc3339()),
                        // Already filtered: NoteInfo::from drops the daemon's
                        // own stamps, so this is the author's frontmatter.
                        "properties": n.properties
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
                // The client DTO (`rpc_client/storage.rs`) reads `wikilinks`,
                // not `links_to`. Both stay: the web reader pins `links_to`.
                "wikilinks": note.links_to.iter()
                    .map(|t| serde_json::json!({ "target": t }))
                    .collect::<Vec<_>>(),
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
    use crucible_core::config::KilnName;
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

    /// The client-side `DaemonStorageClient` reads `wikilinks` from this
    /// reply. The daemon wrote only `links_to`, so every note came back with
    /// no links. The test parses the reply with the same DTO the client uses.
    #[tokio::test]
    async fn get_note_by_name_reply_carries_wikilinks_the_client_reads() {
        let tmp = TempDir::new().unwrap();
        let kiln_dir = tmp.path();
        std::fs::write(kiln_dir.join("target.md"), "# Target\n").unwrap();
        let source = kiln_dir.join("source.md");
        std::fs::write(&source, "# Source\n\nSee [[target]].\n").unwrap();

        let km = Arc::new(KilnManager::new());
        km.process_file(kiln_dir, &source).await.unwrap();

        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "get_note_by_name",
            "params": { "kiln": kiln_dir.to_string_lossy(), "name": "source" },
        }))
        .unwrap();
        let resp = handle_get_note_by_name(req, &km).await;
        let data = resp.result.expect("the note exists");

        assert_eq!(
            data["links_to"],
            serde_json::json!(["target"]),
            "the existing field stays for the web reader: {data}"
        );
        assert_eq!(
            data["wikilinks"][0]["target"],
            serde_json::json!("target"),
            "the client DTO reads `wikilinks[].target`: {data}"
        );

        let note = crate::rpc_client::parse_note_from_record(&data)
            .expect("the client DTO parses the reply");
        assert_eq!(note.wikilinks.len(), 1, "the client must see the link");
        assert_eq!(note.wikilinks[0].target, "target");
    }

    fn register_request(name: &str, path: &Path) -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "kiln.register",
            "params": {
                "name": name,
                "path": path.to_string_lossy(),
                "auto": false,
                "make_default": false,
            },
        }))
        .unwrap()
    }

    /// `--kiln <path>` and kiln discovery: a directory, and no name.
    fn unnamed_register_request(path: &Path) -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "kiln.register",
            "params": { "path": path.to_string_lossy() },
        }))
        .unwrap()
    }

    /// A registration with no name asks the daemon to derive one.
    ///
    /// The derivation has to happen here, not in the caller: it depends on
    /// what is already registered, and a caller with its own copy of the
    /// registry derives against a different set.
    #[tokio::test]
    async fn a_registration_with_no_name_gets_one_derived_from_the_basename() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let dir = tmp.path().join("My Vault");
        std::fs::create_dir_all(&dir).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp =
            handle_kiln_register(unnamed_register_request(&dir), &registry, &state, None).await;
        let data = resp.result.expect("an unnamed registration succeeds");

        assert_eq!(data["name"], serde_json::json!("My Vault"));
        assert_eq!(data["outcome"], serde_json::json!("added"));
        assert!(state.read().unwrap().kilns.contains_key("My Vault"));
        assert_eq!(
            registry
                .resolve(&KilnName::parse("my vault").unwrap())
                .path()
                .as_deref(),
            Some(dir.canonicalize().unwrap().as_path()),
        );
    }

    /// Two directories with the same basename. The second gets the
    /// disambiguation, not a refusal and not the first one's entry —
    /// re-pointing `notes` at a second corpus is the failure this avoids.
    #[tokio::test]
    async fn a_derived_name_already_taken_is_disambiguated() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let first = tmp.path().join("a").join("notes");
        let second = tmp.path().join("b").join("notes");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[("notes", &first)]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp =
            handle_kiln_register(unnamed_register_request(&second), &registry, &state, None).await;
        let data = resp.result.expect("the second directory registers");

        assert_eq!(data["name"], serde_json::json!("notes-2"));
        // And `notes` still reaches the directory it always did.
        assert_eq!(
            registry
                .resolve(&KilnName::parse("notes").unwrap())
                .path()
                .as_deref(),
            Some(first.canonicalize().unwrap().as_path()),
        );
    }

    /// Registering a directory that already has a name does not mint a second
    /// one. `--kiln ~/notes` twice is one kiln, not `notes` and `notes-2`.
    #[tokio::test]
    async fn an_unnamed_registration_of_a_known_directory_reuses_its_name() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let dir = tmp.path().join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[("notes", &dir)]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp =
            handle_kiln_register(unnamed_register_request(&dir), &registry, &state, None).await;
        let data = resp.result.expect("a known directory registers as itself");

        assert_eq!(data["name"], serde_json::json!("notes"));
        assert_eq!(
            data["outcome"],
            serde_json::json!("added"),
            "the config layer held the name; the state layer records it too"
        );
    }

    /// `cru kiln register` writes the state file, not the user's config.
    ///
    /// The CLI used to edit `crucible.toml` itself. It now calls this handler,
    /// so the assertion that used to read the config back reads the reply and
    /// the state file the reply names.
    #[tokio::test]
    async fn registering_a_name_records_it_in_the_state_file() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let dir = tmp.path().join("some-directory");
        std::fs::create_dir_all(&dir).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp =
            handle_kiln_register(register_request("work", &dir), &registry, &state, None).await;
        let data = resp.result.expect("the directory exists, so it registers");

        assert_eq!(data["name"], serde_json::json!("work"));
        assert_eq!(data["outcome"], serde_json::json!("added"));
        let state_file = std::path::PathBuf::from(data["state_file"].as_str().unwrap());
        assert!(state_file.is_file(), "the reply names the file it wrote");

        // The running registry resolves it too, not only the file. A name that
        // needs a daemon restart to work is the bug this pairing prevents.
        assert_eq!(
            registry
                .resolve(&KilnName::parse("work").unwrap())
                .path()
                .as_deref(),
            Some(dir.canonicalize().unwrap().as_path()),
        );
    }

    /// A relative path is stored absolute.
    ///
    /// The registration is read back from arbitrary working directories, so a
    /// relative entry points somewhere different every time. `cru init` with no
    /// argument hands the CLI exactly that: ".".
    ///
    /// Owned by `registration.rs` until the wizard's config writer was deleted;
    /// the property belongs wherever the write now happens.
    #[tokio::test]
    async fn a_relative_path_is_registered_absolute() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        // Relative paths anchor at the registry's base, which the fixture sets
        // to `data_home` — so the directory has to be there for the floor to
        // accept it.
        std::fs::create_dir_all(data_home.join("relative-kiln")).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp = handle_kiln_register(
            register_request("here", Path::new("relative-kiln")),
            &registry,
            &state,
            None,
        )
        .await;
        let data = resp.result.expect("a relative path registers");

        let stored = std::path::PathBuf::from(data["path"].as_str().expect("a path"));
        assert!(
            stored.is_absolute(),
            "a relative entry points somewhere different on every run: {stored:?}"
        );
        assert!(state.read().unwrap().kilns["here"].path.is_absolute());
    }

    /// Registering a second kiln must not silently steal the default.
    ///
    /// Also owned by `registration.rs` before its writer was deleted. The rule
    /// is unchanged: only an explicit `make_default`, or an empty slot, sets it.
    #[tokio::test]
    async fn registering_a_second_kiln_leaves_the_existing_default_alone() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let first = tmp.path().join("a");
        let second = tmp.path().join("b");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        handle_kiln_register(register_request("first", &first), &registry, &state, None)
            .await
            .result
            .expect("the first registers");
        handle_kiln_register(register_request("second", &second), &registry, &state, None)
            .await
            .result
            .expect("the second registers");

        assert_eq!(
            state.read().unwrap().default_kiln.as_deref(),
            Some("first"),
            "the second registration must not claim the default"
        );
    }

    /// Names are case-folded, so `Notes` is refused rather than becoming a
    /// second kiln beside `notes` — and the refusal states the rule, because
    /// "invalid kiln name" alone leaves the user guessing.
    #[tokio::test]
    async fn registering_an_out_of_charset_name_is_refused_with_the_rule() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let dir = tmp.path().join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        for bad in ["../escape", ".hidden", "", " padded ", "a/b"] {
            let resp =
                handle_kiln_register(register_request(bad, &dir), &registry, &state, None).await;
            let err = resp
                .error
                .unwrap_or_else(|| panic!("{bad:?} must be refused"));
            assert!(
                err.message.contains("[A-Za-z0-9._- ]"),
                "{bad:?}: the refusal must state the rule, got: {}",
                err.message
            );
        }
        assert!(
            !state.path().exists(),
            "a refused registration writes nothing"
        );
    }

    /// Registering a directory that does not exist is refused: the entry would
    /// be a name resolving to nothing, which is what the whole rule exists to
    /// prevent, and a typo'd path is the ordinary way to produce one.
    #[tokio::test]
    async fn registering_a_missing_directory_is_refused() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp = handle_kiln_register(
            register_request("work", &tmp.path().join("not-there")),
            &registry,
            &state,
            None,
        )
        .await;

        let err = resp.error.expect("a missing directory must not register");
        assert!(err.message.contains("not a directory"), "{}", err.message);
        assert!(!state.path().exists(), "nothing is written");
    }

    /// A name the config already claims for a different directory is refused,
    /// because the config out-ranks the state layer: the entry would be
    /// written and then never used, which is worse than not writing it.
    #[tokio::test]
    async fn registering_over_a_config_declared_name_is_refused() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[("notes", &first)]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp =
            handle_kiln_register(register_request("notes", &second), &registry, &state, None).await;

        let err = resp.error.expect("a claimed name must not be repointed");
        assert!(err.message.contains("notes"), "{}", err.message);
        assert!(!state.path().exists(), "nothing is written");
        // And the name still reaches the directory the config gave it.
        assert_eq!(
            registry
                .resolve(&KilnName::parse("notes").unwrap())
                .path()
                .as_deref(),
            Some(first.canonicalize().unwrap().as_path()),
        );
    }

    fn name_request(method: &str, name: &str) -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": { "name": name },
        }))
        .unwrap()
    }

    fn registry_list_request() -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "kiln.registry_list",
            "params": {},
        }))
        .unwrap()
    }

    /// One row per situation, from the daemon's side of the table. The CLI
    /// renders these; this pins what it renders.
    #[tokio::test]
    async fn the_registry_listing_reports_which_layer_owns_each_name() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let declared = tmp.path().join("declared");
        let shadowed_by_config = tmp.path().join("shadowed");
        let registered = tmp.path().join("registered");
        let agreed = tmp.path().join("agreed");
        let opened = tmp.path().join("opened-by-path");
        for dir in [
            &declared,
            &shadowed_by_config,
            &registered,
            &agreed,
            &opened,
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }

        // The state layer, written before the registry reads it — which is the
        // order the daemon does it in at bind.
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));
        state
            .register(
                &KilnName::parse("only-state").unwrap(),
                &registered,
                false,
                false,
            )
            .unwrap();
        state
            .register(&KilnName::parse("both").unwrap(), &agreed, false, false)
            .unwrap();
        state
            .register(
                &KilnName::parse("contested").unwrap(),
                &shadowed_by_config,
                false,
                false,
            )
            .unwrap();
        // A registration whose directory is gone.
        let vanished = tmp.path().join("vanished");
        std::fs::create_dir_all(&vanished).unwrap();
        state
            .register(&KilnName::parse("gone").unwrap(), &vanished, false, false)
            .unwrap();
        std::fs::remove_dir_all(&vanished).unwrap();

        let registry = crate::test_support::kiln_registry(
            &data_home,
            &[
                ("only-config", &declared),
                ("both", &agreed),
                ("contested", &declared),
            ],
        );
        registry.overlay_state(state.registrations());

        // And one directory open by path, which no registration claims.
        let km = Arc::new(KilnManager::new());
        km.open(&opened).await.expect("open the kiln");

        let resp = handle_kiln_registry_list(
            registry_list_request(),
            &registry,
            &state,
            &km,
            Some("only-config"),
            &data_home,
        )
        .await;
        let data = resp.result.expect("the listing returns rows");
        let rows = data["kilns"].as_array().expect("an array").clone();
        let by_name = |name: &str| {
            rows.iter()
                .find(|row| row["name"] == serde_json::json!(name))
                .unwrap_or_else(|| panic!("no row for {name}: {rows:?}"))
                .clone()
        };

        let only_config = by_name("only-config");
        assert_eq!(only_config["origin"], "config");
        assert_eq!(
            only_config["default"], true,
            "the config layer's `default_kiln` out-ranks the state layer's"
        );
        assert_eq!(only_config["also_registered"], false);

        assert_eq!(by_name("only-state")["origin"], "registered");

        let both = by_name("both");
        assert_eq!(both["origin"], "config");
        assert_eq!(
            both["also_registered"], true,
            "one registration written down twice is not a conflict: {both}"
        );
        assert_eq!(both["shadows"], serde_json::Value::Null);

        let contested = by_name("contested");
        assert_eq!(contested["origin"], "config");
        assert_eq!(
            contested["path"],
            declared.to_string_lossy().as_ref(),
            "the config wins the name"
        );
        assert_eq!(
            contested["shadows"],
            shadowed_by_config.to_string_lossy().as_ref(),
            "the losing path must be named, or the user cannot act on it"
        );

        assert_eq!(by_name("gone")["missing"], true);

        let discovered = by_name("opened-by-path");
        assert_eq!(
            discovered["origin"], "discovered",
            "a directory some other door opened is NOT a kiln a session can name"
        );
    }

    /// `forget` on a config-declared name refuses, and names the file to edit.
    /// There is nothing in the state store to remove, and telling the user
    /// "not found" would send them looking in the wrong place.
    #[tokio::test]
    async fn forgetting_a_config_declared_kiln_refuses_and_names_the_config_file() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let dir = tmp.path().join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = tmp.path().join("config.toml");
        let registry = crate::test_support::kiln_registry(&data_home, &[("notes", &dir)]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp = handle_kiln_forget(
            name_request("kiln.forget", "notes"),
            &registry,
            &state,
            Some(&config_path),
        )
        .await;

        let err = resp.error.expect("a config-declared name must be refused");
        assert!(
            err.message.contains("config.toml"),
            "the refusal must name the declaring file: {}",
            err.message
        );
        // And the name still resolves: a refused forget removes nothing.
        assert!(registry
            .resolve(&KilnName::parse("notes").unwrap())
            .registered()
            .is_some());
    }

    /// A Lua-declared kiln upgrades the refusal from file-only to
    /// `file:line`: the boot provenance knows the exact `cru.config.set`
    /// call site, so the user is sent to the declaration, not the file.
    #[tokio::test]
    async fn forgetting_a_lua_declared_kiln_names_the_call_site() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let dir = tmp.path().join("notes");
        std::fs::create_dir_all(&dir).unwrap();

        // The boot store as the daemon's evaluation leaves it: a kilns entry
        // merged during the boot phase with a Lua call-site tag, location
        // keys stripped from the value afterwards — provenance survives.
        crucible_lua::begin_boot_store();
        crucible_lua::merge_app_config_tagged(
            serde_json::json!({ "kilns": { "notes": dir.to_string_lossy() } }),
            crucible_core::config::ConfigSource::Lua {
                last_set: crucible_core::config::LastSet::new(
                    crucible_core::lua_source::LuaSource::UserLua,
                    "init.lua".to_string(),
                    Some(7),
                ),
            },
        );
        crucible_lua::end_boot_phase();

        let registry = crate::test_support::kiln_registry(&data_home, &[("notes", &dir)]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));
        let config_path = tmp.path().join("config.toml");

        let resp = handle_kiln_forget(
            name_request("kiln.forget", "notes"),
            &registry,
            &state,
            Some(&config_path),
        )
        .await;

        let err = resp.error.expect("a declared name must be refused");
        assert!(
            err.message.contains("lua (init.lua:7)"),
            "the refusal must name the call site: {}",
            err.message
        );
    }

    /// The shadowed case is the one where `forget` earns its keep: both layers
    /// claim the name, the config wins, and the state entry sits there doing
    /// nothing. Forgetting it clears the conflict.
    #[tokio::test]
    async fn forgetting_a_shadowed_registration_clears_the_conflict() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let declared = tmp.path().join("declared");
        let shadowed = tmp.path().join("shadowed");
        std::fs::create_dir_all(&declared).unwrap();
        std::fs::create_dir_all(&shadowed).unwrap();

        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));
        state
            .register(&KilnName::parse("notes").unwrap(), &shadowed, false, false)
            .unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[("notes", &declared)]);
        assert_eq!(
            registry.overlay_state(state.registrations()).len(),
            1,
            "precondition: the config must shadow the registration"
        );

        let resp = handle_kiln_forget(
            name_request("kiln.forget", "notes"),
            &registry,
            &state,
            None,
        )
        .await;

        let data = resp
            .result
            .expect("a shadowed registration can be forgotten");
        assert_eq!(data["takes_effect"], "next daemon start");
        assert!(
            state.read().unwrap().kilns.is_empty(),
            "the state entry must be gone"
        );
        // The running daemon still answers to the name — removal waits for the
        // boot freeze, because it changes what a persisted reference means.
        assert!(registry
            .resolve(&KilnName::parse("notes").unwrap())
            .registered()
            .is_some());
    }

    #[tokio::test]
    async fn forgetting_an_unknown_name_says_so() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));

        let resp = handle_kiln_forget(
            name_request("kiln.forget", "nothing"),
            &registry,
            &state,
            None,
        )
        .await;

        let err = resp.error.expect("an unknown name must be refused");
        assert!(err.message.contains("nothing"), "{}", err.message);
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
        let entry = listed
            .as_array()
            .expect("an array")
            .iter()
            .find(|row| row["path"] == serde_json::json!(kiln_dir.to_string_lossy()))
            .unwrap_or_else(|| panic!("the kiln must be listed: {listed}"))
            .clone();
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

    /// The user's own case, end to end and across the file boundary: a
    /// registration written to `kilns.json` under `Crucible Help` is served
    /// under that spelling, answers to `crucible help`, and is marked the
    /// default. The name crosses a JSON file, the state overlay and the
    /// registry, and every one of those keyed by a raw string before.
    #[tokio::test]
    async fn a_registration_with_capitals_and_a_space_survives_the_state_file() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let docs = tmp.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();

        let state = Arc::new(crate::kiln_state::KilnStateStore::new(&data_home));
        state
            .register(
                &KilnName::parse("Crucible Help").unwrap(),
                &docs,
                false,
                /* make_default */ true,
            )
            .expect("the registration must be written");

        // Re-read from disk, as the daemon does at bind.
        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        registry.overlay_state(state.registrations());

        let attached =
            crate::server::session::scope::resolve_scope_kiln("crucible help", &registry)
                .expect("any case of a registered name must attach");
        assert_eq!(
            attached.name().as_str(),
            "Crucible Help",
            "the session stores the spelling its owner registered"
        );
        assert_eq!(attached.path(), docs);

        let km = Arc::new(KilnManager::new().with_kiln_registry(registry.clone()));
        let resp = handle_kiln_registry_list(
            registry_list_request(),
            &registry,
            &state,
            &km,
            /* config_default_kiln */ None,
            &data_home,
        )
        .await;
        let data = resp.result.expect("the listing returns rows");
        let rows = data["kilns"].as_array().expect("an array");
        // Beside the bundled help corpus, which every config is offered.
        let row = rows
            .iter()
            .find(|row| row["name"] == serde_json::json!("Crucible Help"))
            .unwrap_or_else(|| panic!("no row under the registered spelling: {rows:?}"));
        assert_eq!(row["origin"], "registered");
        assert_eq!(row["default"], true);
    }

    /// A directory opened under any door gets its name from the registry,
    /// because the registry is the only thing that can hand a name back to
    /// `session.connect_kiln`. `kiln.open` used to have no registration floor
    /// and no entry, and the listing filled the hole with the basename it
    /// *would* have derived — a label the attach then refused with 422.
    #[tokio::test]
    async fn an_open_kiln_is_named_by_the_registry_that_opened_it() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let kiln_dir = tmp.path().join("My Vault");
        std::fs::create_dir_all(&kiln_dir).unwrap();

        let registry = crate::test_support::kiln_registry(&data_home, &[]);
        let km = Arc::new(KilnManager::new().with_kiln_registry(registry.clone()));
        assert!(
            registry.name_for(&kiln_dir).is_none(),
            "precondition: the directory must be unregistered"
        );

        km.open(&kiln_dir).await.expect("open the kiln");

        assert_eq!(
            registry.name_for(&kiln_dir).as_ref().map(|n| n.as_str()),
            Some("My Vault"),
            "opening a directory is what gives it a name"
        );

        let resp = handle_kiln_list(list_request(), &km, &registry, &data_home).await;
        let listed = resp.result.expect("kiln.list returns a list");
        let row = listed
            .as_array()
            .expect("an array")
            .iter()
            .find(|row| row["name"] == serde_json::json!("My Vault"))
            .unwrap_or_else(|| panic!("the opened kiln must be listed: {listed}"));
        assert_eq!(row["registered"], true);
        assert_eq!(row["open"], true);
    }

    /// A registered kiln is reachable after a restart, even a LAZY one.
    ///
    /// `kiln.list` reported the kilns the manager held OPEN, and a fresh daemon
    /// holds none. Every kiln-addressed route gates on that listing — the web
    /// file editor checks the path against it before reading a byte — so a
    /// restart turned a registered kiln into a 404 and the picker lost it.
    ///
    /// Lazy has to keep meaning lazy: the entry is listed, it is honestly
    /// marked closed, and the first request that addresses it opens it.
    #[tokio::test]
    async fn a_lazy_registered_kiln_is_listed_and_opens_on_first_use() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let vault = tmp.path().join("Team Notes");
        std::fs::create_dir_all(&vault).unwrap();

        // A fresh daemon over the same data root: the registry knows the kiln,
        // the manager has opened nothing.
        let registry = crate::test_support::kiln_registry_with_lazy(
            &data_home,
            &[("Team Notes", &vault, true)],
        );
        let km = Arc::new(KilnManager::new().with_kiln_registry(registry.clone()));

        let listed = handle_kiln_list(list_request(), &km, &registry, &data_home)
            .await
            .result
            .expect("kiln.list returns a list");
        let rows = listed.as_array().expect("an array");
        let row = rows
            .iter()
            .find(|row| row["name"] == serde_json::json!("Team Notes"))
            .unwrap_or_else(|| panic!("a registered kiln must be listed: {listed}"));
        assert_eq!(
            row["open"], false,
            "and listed honestly as closed: {listed}"
        );
        assert_eq!(row["registered"], true);
        assert_eq!(row["path"], vault.to_string_lossy().as_ref());

        // The first request that addresses it opens it. This is the write the
        // web file editor's 404 stood in front of.
        let upsert = handle_note_upsert(
            Request {
                jsonrpc: "2.0".to_string(),
                id: Some(crucible_core::protocol::RequestId::Number(2)),
                method: "note.upsert".to_string(),
                params: serde_json::json!({
                    "kiln": vault.to_string_lossy(),
                    "note": {
                        "path": "Watched.md",
                        "content_hash": crucible_core::parser::BlockHash::zero(),
                        "title": "Watched",
                        "tags": [],
                        "links_to": [],
                        "properties": {},
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                    },
                }),
            },
            &km,
        )
        .await;
        assert!(upsert.error.is_none(), "{:?}", upsert.error);

        let listed = handle_kiln_list(list_request(), &km, &registry, &data_home)
            .await
            .result
            .expect("kiln.list returns a list");
        let row = listed
            .as_array()
            .expect("an array")
            .iter()
            .find(|row| row["name"] == serde_json::json!("Team Notes"))
            .expect("still listed");
        assert_eq!(row["open"], true, "the use opened it: {listed}");
    }

    /// A registered kiln whose directory is gone is NOT offered. An attach
    /// would fail to open it, and the listing's rule is that every name it
    /// publishes is one the attach resolves. `cru kiln list` is the surface
    /// that reports a missing registration, with `(missing)`.
    #[tokio::test]
    async fn a_registered_kiln_with_no_directory_is_not_listed() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let gone = tmp.path().join("gone");
        std::fs::create_dir_all(&gone).unwrap();
        let registry = crate::test_support::kiln_registry(&data_home, &[("gone", &gone)]);
        std::fs::remove_dir_all(&gone).unwrap();

        let km = Arc::new(KilnManager::new().with_kiln_registry(registry.clone()));
        let listed = handle_kiln_list(list_request(), &km, &registry, &data_home)
            .await
            .result
            .expect("kiln.list returns a list");

        // By name, not by count: the bundled help corpus is a registered lazy
        // kiln too, and it is listed whenever its directory exists on the
        // machine running the test.
        assert!(
            !listed
                .as_array()
                .expect("an array")
                .iter()
                .any(|row| row["name"] == serde_json::json!("gone")),
            "a registration with no directory must not be offered: {listed}"
        );
    }

    /// The rule, stated as the round trip the user's 422 broke: every name the
    /// listing publishes is a name the attach resolves. The listing is the
    /// only place a picker learns a name, so a name it invents is a name the
    /// user is invited to send and the daemon then refuses.
    #[tokio::test]
    async fn every_name_kiln_list_publishes_can_be_attached() {
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().join("data");
        let configured = tmp.path().join("Team Notes");
        let by_path = tmp.path().join("docs");
        std::fs::create_dir_all(&configured).unwrap();
        std::fs::create_dir_all(&by_path).unwrap();

        let registry =
            crate::test_support::kiln_registry(&data_home, &[("Team Notes", &configured)]);
        let km = Arc::new(KilnManager::new().with_kiln_registry(registry.clone()));
        km.open(&configured)
            .await
            .expect("open the configured kiln");
        // The door the web client uses, and the one that had no registration.
        km.open(&by_path).await.expect("open the other kiln");

        let resp = handle_kiln_list(list_request(), &km, &registry, &data_home).await;
        let listed = resp.result.expect("kiln.list returns a list");
        // Every row under this test's own temp directory. The bundled help
        // corpus is a registered kiln as well, and whether its directory
        // exists depends on the machine, so counting every row would make this
        // pass or fail for a reason it is not about.
        let rows: Vec<&serde_json::Value> = listed
            .as_array()
            .expect("an array")
            .iter()
            .filter(|row| {
                row["path"]
                    .as_str()
                    .is_some_and(|path| path.starts_with(tmp.path().to_str().unwrap()))
            })
            .collect();
        assert_eq!(rows.len(), 2, "both kilns are listed: {listed}");

        for row in rows {
            let name = row["name"].as_str().expect("a row names its kiln");
            assert!(
                row["registered"].as_bool().unwrap_or(false),
                "an offered name must be a registered one: {row}"
            );
            if let Err(refusal) = crate::server::session::scope::resolve_scope_kiln(name, &registry)
            {
                panic!(
                    "kiln.list published {name:?}, which session.connect_kiln refuses: {refusal}"
                );
            }
        }
    }
}
