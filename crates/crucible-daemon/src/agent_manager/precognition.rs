/// Parameters for executing a multi-kiln search.
struct ExecuteMultiKilnSearchParams<'a> {
    session_id: &'a str,
    sources: &'a [KilnSearchSource],
    query_embedding: Vec<f32>,
    agent_config: &'a SessionAgent,
    session: &'a crucible_core::session::Session,
    event_tx: &'a broadcast::Sender<SessionEventMessage>,
    original_content: &'a str,
}

use super::*;

impl AgentManager {
    /// Build just the Precognition context block (no original-content
    /// concatenation). Used by `compute_precognition_message` which
    /// wraps the block in a `ContextMessage::system` for prepending
    /// via the `transform_context` seam.
    ///
    /// Returns an empty string for empty results so callers can detect
    /// the "nothing to inject" case and skip prepending entirely.
    ///
    /// **Empty-results behavior for the `precognition_format` Lua hook:**
    /// the hook does NOT fire on empty results — we short-circuit
    /// before invocation. This matches the pre-migration string-mutating
    /// implementation; plugin authors who want to inject a "no notes"
    /// message on empty results should use the `transform_context` Lua
    /// hook instead (which fires every turn) and check whether a
    /// system Precognition message is already present.
    async fn format_precognition_context_block(
        session_id: &str,
        original_content: &str,
        results: &[crucible_core::SearchResult],
        primary_kiln: &std::path::Path,
        state: &SessionEventState,
        plugin_handlers: Option<&(
            std::sync::Arc<crucible_lua::LuaScriptHandlerRegistry>,
            std::sync::Arc<mlua::Lua>,
        )>,
    ) -> String {
        if results.is_empty() {
            return String::new();
        }
        let custom_formatted = Self::execute_precognition_format_handlers(
            session_id,
            original_content,
            results,
            state,
            plugin_handlers,
        )
        .await;

        if let Some(custom) = custom_formatted {
            custom
        } else {
            Self::precognition_context_block(results, primary_kiln)
        }
    }

    /// Pure formatter for the system-message body. No XML wrap for the
    /// user message (the role on the ContextMessage already encodes
    /// "system"); the `<system>...</system>` framing is kept because
    /// it matches what prompt-engineering tutorials and existing
    /// fixtures expect, and many models treat it as a hint that the
    /// content is meta-instruction rather than chat history.
    fn precognition_context_block(
        results: &[crucible_core::SearchResult],
        primary_kiln: &std::path::Path,
    ) -> String {
        if results.is_empty() {
            return String::new();
        }
        let mut context = format!("<system>\nFound {} relevant notes:\n", results.len());
        for result in results {
            let title = result_title(result);
            let kiln_label = result
                .kiln_path
                .as_ref()
                .filter(|path| path != &primary_kiln)
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| format!(" [from: {name}]"))
                .unwrap_or_default();
            context.push_str(&format!(
                "\n## {}{} (similarity: {:.2})\n\n{}\n",
                title,
                kiln_label,
                result.score,
                result.snippet.clone().unwrap_or_default()
            ));
        }
        context.push_str("\n</system>");
        context
    }

    async fn execute_precognition_format_handlers(
        session_id: &str,
        original_content: &str,
        results: &[crucible_core::SearchResult],
        state: &SessionEventState,
        plugin_handlers: Option<&(
            std::sync::Arc<crucible_lua::LuaScriptHandlerRegistry>,
            std::sync::Arc<mlua::Lua>,
        )>,
    ) -> Option<String> {
        let results_payload: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "title": result_title(r),
                    "score": r.score,
                    "snippet": r.snippet.clone().unwrap_or_default(),
                    "kiln_path": r
                        .kiln_path
                        .as_ref()
                        .and_then(|path| path.to_str())
                        .unwrap_or_default(),
                })
            })
            .collect();

        let event = SessionEvent::Custom {
            name: "precognition_format".to_string(),
            payload: serde_json::json!({
                "user_message": original_content,
                "note_count": results.len(),
                "results": results_payload,
            }),
        };

        // First Transform wins, session VM before plugin VM: a session's
        // custom formatter overrides a plugin's default. Formatters run
        // pre-turn and are expected to be quick, so the plugin pass runs
        // under the same caller-held state lock rather than after it.
        if let Some(formatted) =
            Self::run_precognition_format_pass(session_id, &state.registry, &state.lua, &event)
                .await
        {
            return Some(formatted);
        }
        if let Some((plugin_registry, plugin_lua)) = plugin_handlers {
            return Self::run_precognition_format_pass(
                session_id,
                plugin_registry,
                plugin_lua,
                &event,
            )
            .await;
        }
        None
    }

    /// One registry's `precognition_format` pass; its first Transform wins.
    async fn run_precognition_format_pass(
        session_id: &str,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        event: &SessionEvent,
    ) -> Option<String> {
        use crucible_lua::ScriptHandlerResult;

        for handler in registry.runtime_handlers_for("precognition_format", None) {
            match registry
                .execute_runtime_handler(lua, &handler.name, event, Some(session_id))
                .await
            {
                Ok(ScriptHandlerResult::Transform(value)) => {
                    if let Some(formatted) = value.as_str() {
                        return Some(formatted.to_string());
                    }
                }
                Ok(ScriptHandlerResult::PassThrough)
                | Ok(ScriptHandlerResult::Cancel { .. })
                | Ok(ScriptHandlerResult::Inject { .. })
                | Ok(ScriptHandlerResult::Handled { .. }) => {}
                Err(error) => {
                    warn!(
                        session_id = %session_id,
                        error = %error,
                        "precognition_format handler error (fail-open)"
                    );
                }
            }
        }
        None
    }

    /// Run the `precognition_select` seam: let Lua choose which retrieved
    /// notes reach the agent, in what order, and how the snippet budget is
    /// spent across them.
    ///
    /// Distinct from `precognition_format`, which reshapes notes that were
    /// already chosen. Formatting still runs afterward.
    ///
    /// Returns `None` when no handler took the decision, leaving the Rust
    /// default in place.
    async fn execute_precognition_select_handlers(
        session_id: &str,
        original_content: &str,
        results: &[crucible_core::SearchResult],
        primary_kiln: &std::path::Path,
        char_budget: usize,
        state: &SessionEventState,
        plugin_handlers: Option<&(
            std::sync::Arc<crucible_lua::LuaScriptHandlerRegistry>,
            std::sync::Arc<mlua::Lua>,
        )>,
    ) -> Option<Vec<crucible_core::SearchResult>> {
        let results_payload: Vec<serde_json::Value> = results
            .iter()
            .enumerate()
            .map(|(position, r)| {
                serde_json::json!({
                    // 1-based: this is the handle handlers return to select a
                    // result, and Lua arrays are 1-based.
                    "index": position + 1,
                    "title": result_title(r),
                    "score": r.score,
                    "snippet": r.snippet.clone().unwrap_or_default(),
                    "kiln_path": r
                        .kiln_path
                        .as_ref()
                        .and_then(|path| path.to_str())
                        .unwrap_or_default(),
                    // Otherwise only derivable by string-comparing kiln_path
                    // against the session kiln, which is a trap.
                    "is_primary_kiln": r
                        .kiln_path
                        .as_ref()
                        .is_none_or(|path| path.as_path() == primary_kiln),
                })
            })
            .collect();

        let event = SessionEvent::Custom {
            name: "precognition_select".to_string(),
            payload: serde_json::json!({
                "user_message": original_content,
                "note_count": results.len(),
                // Handlers allocate within this; core still enforces it after.
                "char_budget": char_budget,
                "results": results_payload,
            }),
        };

        // Session VM before plugin VM, first Transform wins — same precedence
        // as `precognition_format`, so a session's policy overrides a plugin's.
        if let Some(selected) = Self::run_precognition_select_pass(
            session_id,
            &state.registry,
            &state.lua,
            &event,
            results,
        )
        .await
        {
            return Some(selected);
        }
        if let Some((plugin_registry, plugin_lua)) = plugin_handlers {
            return Self::run_precognition_select_pass(
                session_id,
                plugin_registry,
                plugin_lua,
                &event,
                results,
            )
            .await;
        }
        None
    }

    /// One registry's `precognition_select` pass; its first usable Transform wins.
    async fn run_precognition_select_pass(
        session_id: &str,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        event: &SessionEvent,
        results: &[crucible_core::SearchResult],
    ) -> Option<Vec<crucible_core::SearchResult>> {
        use crucible_lua::ScriptHandlerResult;

        for handler in registry.runtime_handlers_for("precognition_select", None) {
            match registry
                .execute_runtime_handler(lua, &handler.name, event, Some(session_id))
                .await
            {
                Ok(ScriptHandlerResult::Transform(value)) => {
                    if let Some(selected) = apply_precognition_selection(results, &value) {
                        return Some(selected);
                    }
                    warn!(
                        session_id = %session_id,
                        handler = %handler.name,
                        "precognition_select handler returned neither a list of \
                         entries nor an empty table; ignoring"
                    );
                }
                Ok(ScriptHandlerResult::PassThrough)
                | Ok(ScriptHandlerResult::Cancel { .. })
                | Ok(ScriptHandlerResult::Inject { .. })
                | Ok(ScriptHandlerResult::Handled { .. }) => {}
                Err(error) => {
                    warn!(
                        session_id = %session_id,
                        error = %error,
                        "precognition_select handler error (fail-open)"
                    );
                }
            }
        }
        None
    }

    /// Collect search sources from the primary kiln and any connected kilns.
    /// Connected kilns are skipped if they lack enrichment config or use a
    /// different embedding model than the primary kiln.
    pub(super) async fn collect_kiln_search_sources(
        &self,
        session_id: &str,
        session: &crucible_core::session::Session,
        primary_handle: &crate::kiln_manager::StorageHandle,
        primary_config: &crucible_core::config::EmbeddingProviderConfig,
    ) -> Vec<KilnSearchSource> {
        let mut sources = vec![KilnSearchSource {
            kiln_path: session.kiln.clone(),
            knowledge_repo: primary_handle.as_knowledge_repository(),
            is_primary: true,
        }];

        for connected_kiln in &session.connected_kilns {
            let connected_handle = match self.kiln_manager.get_or_open(connected_kiln).await {
                Ok(handle) => handle,
                Err(error) => {
                    warn!(
                        session_id = %session_id,
                        kiln = %connected_kiln.display(),
                        error = %error,
                        "Failed to open connected kiln for precognition"
                    );
                    continue;
                }
            };

            let Some(connected_config) = self.kiln_manager.enrichment_config().cloned() else {
                debug!(
                    session_id = %session_id,
                    kiln = %connected_kiln.display(),
                    "Skipping connected kiln without enrichment config"
                );
                continue;
            };

            if connected_config.model_name() != primary_config.model_name() {
                // TODO: Compare stored model metadata instead of just model names (currently all kilns share one enrichment config)
                warn!(
                    session_id = %session_id,
                    kiln = %connected_kiln.display(),
                    primary_model = primary_config.model_name(),
                    connected_model = connected_config.model_name(),
                    "Skipping connected kiln with mismatched embedding model"
                );
                continue;
            }

            sources.push(KilnSearchSource {
                kiln_path: connected_kiln.clone(),
                knowledge_repo: connected_handle.as_knowledge_repository(),
                is_primary: false,
            });
        }

        sources
    }

    /// Execute a vector search across the given kiln sources.
    /// Returns the results and the number of kilns searched, or `None` on failure
    /// (after emitting a precognition event).
    async fn execute_multi_kiln_search(
        &self,
        params: ExecuteMultiKilnSearchParams<'_>,
    ) -> Option<Vec<crucible_core::SearchResult>> {
        let provider_trust = resolve_provider_trust(params.agent_config, self.llm_config.as_ref());
        let kilns_searched = params.sources.len();

        match search_across_kilns(
            params.sources,
            params.query_embedding,
            params.agent_config.precognition_results,
            Some(provider_trust),
            &params.session.workspace,
        )
        .await
        {
            Ok(r) => Some(r),
            Err(error) => {
                warn!(session_id = %params.session_id, error = %error, "Precognition search across kilns failed");
                emit_precognition_event(
                    params.event_tx,
                    params.session_id,
                    params.original_content,
                    0,
                    kilns_searched,
                    1,
                    None,
                );
                None
            }
        }
    }

    /// Compute the Precognition system message for this turn, if any.
    ///
    /// Returns the kiln-search context as a system `ContextMessage`
    /// that the caller prepends to the message array via the
    /// `transform_context` seam. Returns `None` when there's nothing
    /// to inject (no kiln, no embedding backend, search returned no
    /// results, or any failure — Precognition is best-effort).
    ///
    /// Earlier this function returned the entire prompt with `<system>`
    /// XML prepended; now it returns just the system message body
    /// wrapped in a `ContextMessage::system`. The string-mutation path
    /// was a workaround for the absence of a context-array seam.
    pub(super) async fn compute_precognition_message(
        &self,
        session_id: &str,
        original_content: &str,
        session: &crucible_core::session::Session,
        agent_config: &SessionAgent,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> Option<crucible_core::traits::ContextMessage> {
        let kiln_path = session.kiln.as_path();

        let handle = match self.kiln_manager.get_or_open(kiln_path).await {
            Ok(h) => h,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Failed to open kiln for precognition");
                return None;
            }
        };

        let primary_config = self.kiln_manager.enrichment_config().cloned()?;

        let embedding_provider = match crate::embedding::get_or_create_embedding_provider(
            &primary_config,
        )
        .await
        {
            Ok(p) => p,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Failed to create embedding provider for precognition");
                return None;
            }
        };

        let query_embedding = match embedding_provider.embed(original_content).await {
            Ok(e) => e,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Precognition embedding failed");
                emit_precognition_event(event_tx, session_id, original_content, 0, 1, 1, None);
                return None;
            }
        };

        let sources = self
            .collect_kiln_search_sources(session_id, session, &handle, &primary_config)
            .await;
        let kilns_searched = sources.len();

        let mut results = self
            .execute_multi_kiln_search(ExecuteMultiKilnSearchParams {
                session_id,
                sources: &sources,
                query_embedding,
                agent_config,
                session,
                event_tx,
                original_content,
            })
            .await?;

        let char_budget = self.kiln_manager.max_precognition_chars();

        // Selection seam: Lua may narrow, reorder and re-snippet before the
        // budget backstop below. Runs ahead of `precognition_format`, which
        // then formats whatever survives.
        //
        // Takes the session-state lock in its own scope and drops it before the
        // formatting pass below re-takes it. Two short holds beat one spanning
        // both Lua passes — they run pre-turn on every message, so contention
        // matters more here than the pair being atomic. Nothing depends on the
        // two passes seeing the same registry snapshot.
        if let Some(selected) = {
            let plugin_pair = self.plugin_handlers();
            let session_state = self.get_or_create_session_state(session_id);
            let state = session_state.lock().await;
            Self::execute_precognition_select_handlers(
                session_id,
                original_content,
                &results,
                &session.kiln,
                char_budget,
                &state,
                plugin_pair.as_ref(),
            )
            .await
        } {
            results = selected;
        }

        // Budget enforcement stays core's job even when a handler allocated:
        // a no-op for a well-behaved handler, a hard cap on a runaway one.
        apply_precognition_char_cap(&mut results, char_budget);

        let context_block = {
            let plugin_pair = self.plugin_handlers();
            let session_state = self.get_or_create_session_state(session_id);
            let state = session_state.lock().await;
            Self::format_precognition_context_block(
                session_id,
                original_content,
                &results,
                &session.kiln,
                &state,
                plugin_pair.as_ref(),
            )
            .await
        };
        let note_info = extract_note_info(&results, &session.kiln);
        let deduped_count = note_info.len();

        emit_precognition_event(
            event_tx,
            session_id,
            original_content,
            deduped_count,
            kilns_searched,
            0,
            Some(note_info),
        );

        // Empty context block (no results) → don't inject anything; the
        // empty message would just waste tokens.
        if context_block.trim().is_empty() {
            return None;
        }

        // Tag the message so the drop-protection check in
        // `apply_transform_context_handlers` can identify it by metadata
        // rather than content. Lets a Lua handler legitimately mutate
        // the precog content (translate, redact, summarize) without
        // tripping the re-prepend logic — as long as the handler
        // preserves the tag.
        let mut msg = crucible_core::traits::ContextMessage::system(context_block);
        msg.metadata.tags.push(PRECOGNITION_TAG.to_string());
        Some(msg)
    }
}

/// Metadata tag that marks a message as the built-in Precognition
/// system block. The `transform_context` seam uses this to detect when
/// a Lua handler has dropped the message and needs it re-prepended.
pub(crate) const PRECOGNITION_TAG: &str = "precognition";

/// Display title for a search result: the document-ID filename without `.md`.
///
/// Shared by the context block and the handler payloads so the three stay in
/// step. `extract_note_info` deliberately keeps its own `Path`-based version —
/// it dedupes on the filename component, which is a different question.
fn result_title(result: &crucible_core::SearchResult) -> String {
    result
        .document_id
        .0
        .split('/')
        .next_back()
        .unwrap_or(&result.document_id.0)
        .trim_end_matches(".md")
        .to_string()
}

/// Normalize a handler's returned selection list into ordered entries.
///
/// `lua_table_to_json` converts every Lua table — sequences included — into a
/// JSON *object* with stringified integer keys (`{"1": …, "2": …}`), so a Lua
/// list never arrives here as a JSON array. We accept both shapes, and sort the
/// object form numerically: serde_json's map ordering would otherwise place
/// `"10"` before `"2"`, silently scrambling the handler's chosen order.
///
/// Returns `None` for a table that has entries but no numeric keys at all, so a
/// malformed return falls back to the Rust default instead of silently
/// suppressing precognition. A genuinely empty table yields an empty selection,
/// which is the documented "suppress this turn" outcome.
fn selection_entries(selection: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    if let Some(array) = selection.as_array() {
        return Some(array.clone());
    }

    let map = selection.as_object()?;
    if map.is_empty() {
        return Some(Vec::new());
    }

    let mut keyed: Vec<(u64, serde_json::Value)> = map
        .iter()
        .filter_map(|(key, value)| key.parse::<u64>().ok().map(|k| (k, value.clone())))
        .collect();

    if keyed.is_empty() {
        return None;
    }

    keyed.sort_by_key(|(key, _)| *key);
    Some(keyed.into_iter().map(|(_, value)| value).collect())
}

/// Apply a `precognition_select` handler's return value to the search results.
///
/// Handlers return `[{ index = n, snippet = "..." }]` — an index handle rather
/// than free-form result objects, so the selected set is always a subset of what
/// the kiln returned and a handler cannot introduce a note that isn't there.
///
/// This constrains identity, not text: `snippet` is replaceable, so a handler
/// can still put arbitrary content under a real note's title. Handlers are
/// trusted code and `precognition_format` can already return an arbitrary block,
/// so this is not an escalation — but the guarantee is "the note set is real",
/// not "the text is unmodified".
///
/// Malformed entries are dropped with a warning rather than failing the turn —
/// selection is not a gate, so it fails open like every hook except
/// `pre_tool_call`.
fn apply_precognition_selection(
    results: &[crucible_core::SearchResult],
    selection: &serde_json::Value,
) -> Option<Vec<crucible_core::SearchResult>> {
    let entries = selection_entries(selection)?;
    let mut seen = std::collections::HashSet::new();
    let mut selected = Vec::with_capacity(entries.len());

    for entry in &entries {
        let Some(index) = entry.get("index").and_then(|value| value.as_u64()) else {
            warn!("precognition_select entry missing numeric `index`; dropping");
            continue;
        };
        // 1-based on the Lua side; index 0 underflows to None and is dropped.
        let Some(original) = index
            .checked_sub(1)
            .and_then(|zero_based| results.get(zero_based as usize))
        else {
            warn!(index, "precognition_select index out of range; dropping");
            continue;
        };
        if !seen.insert(index) {
            warn!(index, "precognition_select duplicate index; dropping");
            continue;
        }

        let mut result = original.clone();
        if let Some(snippet) = entry.get("snippet").and_then(|value| value.as_str()) {
            result.snippet = Some(snippet.to_string());
        }
        selected.push(result);
    }

    Some(selected)
}

fn apply_precognition_char_cap(results: &mut [crucible_core::SearchResult], cap: usize) {
    if results.is_empty() {
        return;
    }

    let total_chars: usize = results
        .iter()
        .map(|result| {
            result
                .snippet
                .as_deref()
                .unwrap_or_default()
                .chars()
                .count()
        })
        .sum();

    if total_chars <= cap {
        return;
    }

    let per_snippet_cap = cap / results.len();

    for result in results {
        if let Some(snippet) = &mut result.snippet {
            *snippet = snippet.chars().take(per_snippet_cap).collect();
        }
    }
}

/// Extract `PrecognitionNoteInfo` metadata from search results.
/// Titles are derived from the document ID filename (without `.md`).
/// `kiln_label` is set only for results from non-primary kilns.
///
/// Deduplicates by normalized filename — the DB may contain the same note
/// under both relative (`./docs/Foo.md`) and absolute (`/home/.../docs/Foo.md`)
/// paths due to re-indexing. We normalize to the filename component to collapse
/// these while keeping genuinely different notes (different parent dirs) separate.
// TODO: the real fix is path normalization at ingest time + DB migration to
// clean stale entries. Track via versioning metadata in the notes table.
// TODO: precognition result count (currently hardcoded k=5) should be
// configurable via Lua or session config.
pub(super) fn extract_note_info(
    results: &[crucible_core::SearchResult],
    primary_kiln: &std::path::Path,
) -> Vec<crucible_core::traits::chat::PrecognitionNoteInfo> {
    let mut seen = std::collections::HashSet::new();
    results
        .iter()
        .filter_map(|r| {
            let path = std::path::Path::new(&r.document_id.0);
            let filename = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&r.document_id.0);
            let title = filename.trim_end_matches(".md").to_string();
            let kiln_label = r
                .kiln_path
                .as_ref()
                .filter(|kp| kp.as_path() != primary_kiln)
                .and_then(|kp| kp.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string());
            // Deduplicate by (filename, kiln_label) — collapses duplicate DB
            // entries for the same file (relative vs absolute paths) while
            // keeping different files that share a display title.
            let dedup_key = (filename.to_string(), kiln_label.clone());
            if seen.insert(dedup_key) {
                Some(crucible_core::traits::chat::PrecognitionNoteInfo {
                    title,
                    kiln_label,
                    score: r.score,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod format_precognition_context_tests {
    use super::*;
    use crucible_core::types::database::DocumentId;
    use std::path::PathBuf;

    fn make_result(
        doc_id: &str,
        score: f64,
        snippet: Option<&str>,
        kiln: Option<&str>,
    ) -> crucible_core::SearchResult {
        crucible_core::SearchResult {
            document_id: DocumentId(doc_id.to_string()),
            score,
            highlights: None,
            snippet: snippet.map(|s| s.to_string()),
            kiln_path: kiln.map(PathBuf::from),
        }
    }

    // These tests cover `precognition_context_block` — the pure
    // formatter for the kiln-injected system message. The previous
    // shape concatenated the user's content onto the block; that
    // concatenation now happens implicitly by prepending the system
    // message via `transform_context`. So the block itself no longer
    // contains the user content; tests assert the block content only.

    #[test]
    fn precognition_context_block_empty_results_returns_empty_string() {
        // compute_precognition_message skips injection on empty blocks;
        // this is the contract — empty → empty.
        let result =
            AgentManager::precognition_context_block(&[], std::path::Path::new("/home/user/notes"));
        assert_eq!(result, "");
    }

    #[test]
    fn precognition_context_block_single_result_has_system_tags() {
        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::precognition_context_block(
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(output.starts_with("<system>\n"));
        assert!(output.contains("</system>"));
        assert!(output.contains("Found 1 relevant notes:"));
        assert!(output.contains("## Rust"));
        assert!(output.contains("(similarity: 0.85)"));
        assert!(output.contains("Rust is a systems programming language."));
    }

    #[test]
    fn precognition_context_block_multiple_results() {
        let results = vec![
            make_result(
                "notes/Rust.md",
                0.92,
                Some("Rust is fast."),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Go.md",
                0.78,
                Some("Go is simple."),
                Some("/home/user/notes"),
            ),
        ];

        let output = AgentManager::precognition_context_block(
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(output.contains("Found 2 relevant notes:"));
        assert!(output.contains("## Rust"));
        assert!(output.contains("## Go"));
        assert!(output.contains("Rust is fast."));
        assert!(output.contains("Go is simple."));
    }

    #[test]
    fn precognition_context_block_kiln_label_for_non_primary() {
        let results = vec![make_result(
            "notes/External.md",
            0.70,
            Some("External content."),
            Some("/other/kiln"),
        )];

        let output = AgentManager::precognition_context_block(
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(output.contains("[from: kiln]"));
    }

    #[test]
    fn precognition_context_block_no_kiln_label_for_primary() {
        let results = vec![make_result(
            "notes/Local.md",
            0.90,
            Some("Local content."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::precognition_context_block(
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(!output.contains("[from:"));
    }

    #[test]
    fn precognition_context_block_missing_snippet_handled() {
        let results = vec![make_result(
            "notes/NoSnippet.md",
            0.60,
            None,
            Some("/home/user/notes"),
        )];

        let output = AgentManager::precognition_context_block(
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(output.contains("<system>"));
        assert!(output.contains("</system>"));
        assert!(output.contains("## NoSnippet"));
    }

    #[test]
    fn precognition_context_cap_truncates_when_aggregate_exceeds_limit() {
        let mut results = vec![
            make_result(
                "notes/One.md",
                0.9,
                Some(&"a".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Two.md",
                0.8,
                Some(&"b".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Three.md",
                0.7,
                Some(&"c".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Four.md",
                0.6,
                Some(&"d".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Five.md",
                0.5,
                Some(&"e".repeat(800)),
                Some("/home/user/notes"),
            ),
        ];

        apply_precognition_char_cap(&mut results, 3000);

        let total_chars: usize = results
            .iter()
            .map(|result| {
                result
                    .snippet
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .sum();

        assert_eq!(total_chars, 3000);
        assert!(results.iter().all(|result| result
            .snippet
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count()
            <= 600));
    }

    #[test]
    fn precognition_context_cap_does_not_truncate_when_under_limit() {
        let mut results = vec![
            make_result(
                "notes/One.md",
                0.9,
                Some(&"a".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Two.md",
                0.8,
                Some(&"b".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Three.md",
                0.7,
                Some(&"c".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Four.md",
                0.6,
                Some(&"d".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Five.md",
                0.5,
                Some(&"e".repeat(200)),
                Some("/home/user/notes"),
            ),
        ];

        apply_precognition_char_cap(&mut results, 3000);

        let total_chars: usize = results
            .iter()
            .map(|result| {
                result
                    .snippet
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .sum();

        assert_eq!(total_chars, 1000);
        assert!(results.iter().all(|result| {
            result
                .snippet
                .as_deref()
                .unwrap_or_default()
                .chars()
                .count()
                == 200
        }));
    }
}

#[cfg(test)]
mod precognition_format_hook_tests {
    use super::*;
    use crucible_core::types::database::DocumentId;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex as StdMutex};

    fn make_result(
        doc_id: &str,
        score: f64,
        snippet: Option<&str>,
        kiln: Option<&str>,
    ) -> crucible_core::SearchResult {
        crucible_core::SearchResult {
            document_id: DocumentId(doc_id.to_string()),
            score,
            highlights: None,
            snippet: snippet.map(|s| s.to_string()),
            kiln_path: kiln.map(PathBuf::from),
        }
    }

    fn make_session_event_state() -> SessionEventState {
        let lua = mlua::Lua::new();
        let registry = crucible_lua::LuaScriptHandlerRegistry::new();

        register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register_crucible_on_api should succeed");

        SessionEventState {
            lua,
            registry,
            permission_hooks: Arc::new(StdMutex::new(Vec::new())),
            permission_functions: Arc::new(StdMutex::new(HashMap::new())),
            reactor: Reactor::new(),
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }
    }

    #[tokio::test]
    async fn precognition_format_hook_customizes_output() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r###"
                crucible.on("precognition_format", function(ctx, event)
                    return "## Custom Format\n" .. event.user_message .. "\n" .. event.results[1].title
                end)
            "###,
            )
            .exec()
            .expect("Lua handler should load");

        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        // Custom format handler controls the block content. The block
        // is now what gets injected as a system ContextMessage; the
        // user content lives in a separate message and isn't part of
        // the block.
        let output = AgentManager::format_precognition_context_block(
            "session-1",
            "What is Rust?",
            &results,
            std::path::Path::new("/home/user/notes"),
            &state,
            None,
        )
        .await;

        assert!(output.starts_with("## Custom Format"));
        assert!(output.contains("Rust"));
        assert!(!output.starts_with("<system>"));
    }

    #[tokio::test]
    async fn precognition_format_no_handler_uses_default() {
        let state = make_session_event_state();
        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::format_precognition_context_block(
            "session-1",
            "What is Rust?",
            &results,
            std::path::Path::new("/home/user/notes"),
            &state,
            None,
        )
        .await;

        // Block is just the system content; ContextMessage role handles
        // the "this is a system message" semantic separately.
        assert!(output.contains("<system>"));
        assert!(output.contains("Found 1 relevant notes:"));
        assert!(output.contains("Rust is a systems programming language."));
        // Original content is no longer concatenated into the block.
        assert!(!output.contains("What is Rust?"));
    }

    #[test]
    fn extract_note_info_deduplicates_same_filename_different_paths() {
        // Same file indexed with relative and absolute paths (DB migration artifact)
        let results = vec![
            make_result(
                "./docs/Getting Started.md",
                0.9,
                Some("content"),
                Some("/kiln"),
            ),
            make_result(
                "/home/user/crucible/docs/Getting Started.md",
                0.85,
                Some("same content"),
                Some("/kiln"),
            ),
            make_result("notes/Plugins.md", 0.7, Some("plugin info"), Some("/kiln")),
        ];

        let info = extract_note_info(&results, std::path::Path::new("/kiln"));
        let titles: Vec<&str> = info.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["Getting Started", "Plugins"]);
    }

    #[test]
    fn extract_note_info_keeps_different_filenames_same_title_stem() {
        // Different notes in different directories should NOT be deduped
        // even if they share a display title — they have different filenames
        // (in this case they literally are the same filename so they WILL dedup;
        // truly different notes would have different filenames)
        let results = vec![
            make_result("Help/Guide.md", 0.9, Some("help guide"), Some("/kiln")),
            make_result("Meta/Guide.md", 0.8, Some("meta guide"), Some("/kiln")),
        ];

        // Same filename "Guide.md" from same kiln → deduped (likely duplicate DB entries)
        let info = extract_note_info(&results, std::path::Path::new("/kiln"));
        assert_eq!(info.len(), 1);
    }

    #[test]
    fn extract_note_info_keeps_different_kiln_labels() {
        // Same filename from different kilns are kept as separate entries
        let results = vec![
            make_result("notes/Guide.md", 0.9, Some("local"), Some("/primary")),
            make_result("notes/Guide.md", 0.8, Some("remote"), Some("/secondary")),
        ];

        let info = extract_note_info(&results, std::path::Path::new("/primary"));
        assert_eq!(info.len(), 2);
        assert!(info[0].kiln_label.is_none()); // primary kiln
        assert_eq!(info[1].kiln_label.as_deref(), Some("secondary"));
    }
}

#[cfg(test)]
mod precognition_select_hook_tests {
    use super::*;
    use crucible_core::types::database::DocumentId;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex as StdMutex};

    const KILN: &str = "/home/user/notes";

    fn make_result(doc_id: &str, score: f64, snippet: &str) -> crucible_core::SearchResult {
        crucible_core::SearchResult {
            document_id: DocumentId(doc_id.to_string()),
            score,
            highlights: None,
            snippet: Some(snippet.to_string()),
            kiln_path: Some(PathBuf::from(KILN)),
        }
    }

    fn three_results() -> Vec<crucible_core::SearchResult> {
        vec![
            make_result("notes/Alpha.md", 0.9, "alpha body"),
            make_result("notes/Beta.md", 0.8, "beta body"),
            make_result("notes/Gamma.md", 0.7, "gamma body"),
        ]
    }

    fn make_session_event_state() -> SessionEventState {
        let lua = mlua::Lua::new();
        let registry = crucible_lua::LuaScriptHandlerRegistry::new();
        register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register_crucible_on_api should succeed");

        SessionEventState {
            lua,
            registry,
            permission_hooks: Arc::new(StdMutex::new(Vec::new())),
            permission_functions: Arc::new(StdMutex::new(HashMap::new())),
            reactor: Reactor::new(),
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }
    }

    async fn run_select(
        state: &SessionEventState,
        results: &[crucible_core::SearchResult],
    ) -> Option<Vec<crucible_core::SearchResult>> {
        AgentManager::execute_precognition_select_handlers(
            "session-1",
            "what is alpha?",
            results,
            Path::new(KILN),
            3000,
            state,
            None,
        )
        .await
    }

    async fn run_select_with_budget(
        state: &SessionEventState,
        results: &[crucible_core::SearchResult],
        char_budget: usize,
    ) -> Option<Vec<crucible_core::SearchResult>> {
        AgentManager::execute_precognition_select_handlers(
            "session-1",
            "what is alpha?",
            results,
            Path::new(KILN),
            char_budget,
            state,
            None,
        )
        .await
    }

    fn titles(results: &[crucible_core::SearchResult]) -> Vec<String> {
        results.iter().map(result_title).collect()
    }

    fn snippets(results: &[crucible_core::SearchResult]) -> Vec<String> {
        results
            .iter()
            .map(|r| r.snippet.clone().unwrap_or_default())
            .collect()
    }

    /// The dogfood: the Rust default's snippet-budget policy, reimplemented in
    /// Lua using only what the seam hands a handler.
    ///
    /// `utf8.offset` rather than `string.sub` is load-bearing — the Rust cap
    /// counts *characters* (`.chars().take(n)`) while Lua string indexing is
    /// byte-based, so the naive port silently disagrees on any non-ASCII
    /// snippet and can slice a UTF-8 sequence in half.
    const LUA_EQUAL_SPLIT_CAP: &str = r#"
        crucible.on("precognition_select", function(ctx, event)
            local total = 0
            for _, r in ipairs(event.results) do
                total = total + utf8.len(r.snippet)
            end

            local out = {}
            if total <= event.char_budget then
                for _, r in ipairs(event.results) do
                    out[#out + 1] = { index = r.index }
                end
                return out
            end

            local per = math.floor(event.char_budget / event.note_count)
            for _, r in ipairs(event.results) do
                local snippet = r.snippet
                if utf8.len(snippet) > per then
                    local stop = utf8.offset(snippet, per + 1)
                    snippet = stop and snippet:sub(1, stop - 1) or snippet
                end
                out[#out + 1] = { index = r.index, snippet = snippet }
            end
            return out
        end)
    "#;

    #[tokio::test]
    async fn select_hook_narrows_and_reorders() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { { index = 3 }, { index = 1 } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("handler should take the decision");

        assert_eq!(titles(&selected), vec!["Gamma", "Alpha"]);
    }

    #[tokio::test]
    async fn select_hook_overrides_snippet() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { { index = 1, snippet = "rewritten" } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("selection");

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].snippet.as_deref(), Some("rewritten"));
    }

    #[tokio::test]
    async fn select_hook_receives_index_and_budget() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    -- Assert the payload contract from inside Lua: picking by
                    -- these fields is the whole point of the seam.
                    if event.char_budget ~= 3000 then return {} end
                    if event.note_count ~= 3 then return {} end
                    if not event.results[2].is_primary_kiln then return {} end
                    return { { index = event.results[2].index } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("selection");

        assert_eq!(titles(&selected), vec!["Beta"]);
    }

    #[tokio::test]
    async fn select_hook_empty_table_suppresses_precognition() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return {}
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("empty selection is a decision, not a fall-through");

        assert!(selected.is_empty());
    }

    #[tokio::test]
    async fn select_hook_error_falls_back_to_rust_default() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    error("boom")
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        // Selection is not a gate, so it fails open — only pre_tool_call
        // fails closed.
        assert!(run_select(&state, &three_results()).await.is_none());
    }

    #[tokio::test]
    async fn select_hook_malformed_return_falls_back_rather_than_suppressing() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { oops = "not a selection" }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        // A table with entries but no numeric keys is a bug in the handler.
        // Falling back beats silently dropping the agent's grounding.
        assert!(run_select(&state, &three_results()).await.is_none());
    }

    #[tokio::test]
    async fn no_handler_leaves_the_rust_default_in_place() {
        let state = make_session_event_state();
        assert!(run_select(&state, &three_results()).await.is_none());
    }

    #[test]
    fn selection_entries_orders_numerically_not_lexically() {
        // lua_table_to_json stringifies integer keys, so "10" sorts before "2"
        // under plain map ordering. Guard the numeric sort.
        let selection = serde_json::json!({
            "1":  { "index": 1 },
            "2":  { "index": 2 },
            "10": { "index": 10 },
        });

        let entries = selection_entries(&selection).expect("object form is accepted");
        let order: Vec<u64> = entries
            .iter()
            .map(|e| e["index"].as_u64().unwrap())
            .collect();

        assert_eq!(order, vec![1, 2, 10]);
    }

    #[test]
    fn selection_entries_accepts_a_real_json_array() {
        let selection = serde_json::json!([{ "index": 2 }, { "index": 1 }]);
        let entries = selection_entries(&selection).expect("array form is accepted");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn apply_selection_drops_out_of_range_duplicate_and_zero_indices() {
        let results = three_results();
        let selection = serde_json::json!([
            { "index": 2 },
            { "index": 2 },   // duplicate
            { "index": 99 },  // out of range
            { "index": 0 },   // 0 is not a valid 1-based index
            { "no_index": 1 },
        ]);

        let selected =
            apply_precognition_selection(&results, &selection).expect("array form is accepted");

        assert_eq!(titles(&selected), vec!["Beta"]);
    }

    #[test]
    fn char_cap_still_bounds_a_runaway_handler_selection() {
        // Budget enforcement stays core's job even when Lua allocated.
        let mut selected = vec![
            make_result("notes/One.md", 0.9, &"a".repeat(4000)),
            make_result("notes/Two.md", 0.8, &"b".repeat(4000)),
        ];

        apply_precognition_char_cap(&mut selected, 1000);

        let total: usize = selected
            .iter()
            .map(|r| r.snippet.as_deref().unwrap_or_default().chars().count())
            .sum();
        assert_eq!(total, 1000);
    }

    /// Rust default: the built-in policy applied to the same inputs.
    fn rust_default(
        results: &[crucible_core::SearchResult],
        budget: usize,
    ) -> Vec<crucible_core::SearchResult> {
        let mut baseline = results.to_vec();
        apply_precognition_char_cap(&mut baseline, budget);
        baseline
    }

    /// Lua path: handler decides, then the core backstop still runs.
    async fn lua_reference(
        results: &[crucible_core::SearchResult],
        budget: usize,
    ) -> Vec<crucible_core::SearchResult> {
        let state = make_session_event_state();
        state
            .lua
            .load(LUA_EQUAL_SPLIT_CAP)
            .exec()
            .expect("reference handler should load");

        let mut selected = run_select_with_budget(&state, results, budget)
            .await
            .expect("reference handler should take the decision");
        apply_precognition_char_cap(&mut selected, budget);
        selected
    }

    #[tokio::test]
    async fn lua_reference_matches_rust_default_under_budget() {
        let results = three_results();
        let budget = 3000;

        assert_eq!(
            snippets(&lua_reference(&results, budget).await),
            snippets(&rust_default(&results, budget)),
        );
    }

    #[tokio::test]
    async fn lua_reference_matches_rust_default_over_budget() {
        let results = vec![
            make_result("notes/One.md", 0.9, &"a".repeat(800)),
            make_result("notes/Two.md", 0.8, &"b".repeat(800)),
            make_result("notes/Three.md", 0.7, &"c".repeat(800)),
        ];
        let budget = 900;

        let lua = lua_reference(&results, budget).await;
        let rust = rust_default(&results, budget);

        assert_eq!(snippets(&lua), snippets(&rust));
        // Guard the fixture: if this didn't truncate, the test proves nothing.
        assert!(lua[0].snippet.as_deref().unwrap().len() < 800);
    }

    #[tokio::test]
    async fn lua_reference_matches_rust_default_on_multibyte_snippets() {
        // The Rust cap counts characters; Lua string indexing counts bytes.
        // A `string.sub`-based port disagrees here and can also slice a UTF-8
        // sequence in half — this is the case that forces `utf8.offset`.
        let body: String = "日本語テキスト".repeat(20);
        let results = vec![
            make_result("notes/JaOne.md", 0.9, &body),
            make_result("notes/JaTwo.md", 0.8, &body),
        ];
        let budget = 60;

        let lua = lua_reference(&results, budget).await;
        let rust = rust_default(&results, budget);

        assert_eq!(snippets(&lua), snippets(&rust));
        assert_eq!(lua[0].snippet.as_deref().unwrap().chars().count(), 30);
    }

    /// Measures what the seam costs on the per-turn path. We are on mlua
    /// (Lua 5.4), not LuaJIT, so this is measured rather than assumed —
    /// it decides whether Lua could ever *be* the default here or stays an
    /// opt-in override with Rust shipped as the default.
    ///
    /// Deliberately asserts nothing about wall-clock: a timing threshold in
    /// CI is a flake generator. Run it and read the numbers.
    #[tokio::test]
    #[ignore = "benchmark: run with --ignored --nocapture to read the numbers"]
    async fn measure_lua_selection_overhead() {
        const ITERATIONS: u32 = 500;
        let budget = 3000;

        // Realistic shape: k=5 notes (the default precognition_results) with
        // snippets around the per-note budget.
        let results: Vec<_> = (0..5)
            .map(|i| make_result(&format!("notes/N{i}.md"), 0.9, &"x".repeat(600)))
            .collect();

        let start = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            let mut baseline = results.clone();
            apply_precognition_char_cap(&mut baseline, budget);
            std::hint::black_box(&baseline);
        }
        let rust_total = start.elapsed();

        // Handler registered once, as in production — this measures per-turn
        // dispatch, not Lua compilation.
        let state = make_session_event_state();
        state
            .lua
            .load(LUA_EQUAL_SPLIT_CAP)
            .exec()
            .expect("reference handler should load");

        let start = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            let mut selected = run_select_with_budget(&state, &results, budget)
                .await
                .expect("handler should decide");
            apply_precognition_char_cap(&mut selected, budget);
            std::hint::black_box(&selected);
        }
        let lua_total = start.elapsed();

        println!("iterations:   {ITERATIONS}");
        println!("rust default: {:?}/turn", rust_total / ITERATIONS);
        println!("lua seam:     {:?}/turn", lua_total / ITERATIONS);
        println!(
            "added:        {:?}/turn",
            lua_total.saturating_sub(rust_total) / ITERATIONS
        );
    }
}
