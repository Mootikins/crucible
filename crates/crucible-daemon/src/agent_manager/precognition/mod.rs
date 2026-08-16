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
        label_kilns: bool,
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
            Self::precognition_context_block(results, label_kilns)
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
        label_kilns: bool,
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
                .filter(|_| label_kilns)
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

    /// Collect one search source per kiln the session reaches.
    ///
    /// A kiln that will not open is dropped with a warning rather than failing
    /// the fan-out: one unreadable corpus must not cost the session the rest.
    /// Callers check `enrichment_config()` before calling — all kilns share the
    /// one config, so there is no per-kiln embedding model to reconcile.
    pub(super) async fn collect_kiln_search_sources(
        &self,
        session_id: &str,
        session: &crucible_core::session::Session,
    ) -> Vec<KilnSearchSource> {
        let mut sources = Vec::with_capacity(session.kilns.len());

        for kiln in &session.kilns {
            match self.kiln_manager.get_or_open(kiln).await {
                Ok(handle) => sources.push(KilnSearchSource {
                    kiln_path: kiln.clone(),
                    knowledge_repo: handle.as_knowledge_repository(),
                }),
                Err(error) => warn!(
                    session_id = %session_id,
                    kiln = %kiln.display(),
                    error = %error,
                    "Failed to open kiln for precognition"
                ),
            }
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
        // Every failure below this point emits, because the gate already said
        // Precognition should run for this turn. A silent `None` leaves the
        // transcript unable to say whether the answer was grounded — which is
        // the whole reason `PrecognitionComplete` is persisted rather than
        // being a live-only notification.
        //
        // The one silent path, and deliberately so: no enrichment provider
        // configured is the default state, not a failure. Emitting here would
        // report a grounding failure on the first message of every session that
        // has never configured embeddings.
        let enrichment_config = self.kiln_manager.enrichment_config().cloned()?;

        let embedding_provider = match crate::embedding::get_or_create_embedding_provider(
            &enrichment_config,
        )
        .await
        {
            Ok(p) => p,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Failed to create embedding provider for precognition");
                emit_precognition_event(event_tx, session_id, original_content, 0, 1, 1, None);
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

        let sources = self.collect_kiln_search_sources(session_id, session).await;
        let kilns_searched = sources.len();
        if sources.is_empty() {
            warn!(session_id = %session_id, "No kiln opened for precognition");
            emit_precognition_event(event_tx, session_id, original_content, 0, 0, 1, None);
            return None;
        }

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
        // Name the kiln each note came from only when there is more than one to
        // tell apart. No member of a flat set is the "unlabelled" one, so the
        // choice is per-session, not per-kiln.
        let label_kilns = kilns_searched > 1;

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
                label_kilns,
                &state,
                plugin_pair.as_ref(),
            )
            .await
        };
        let note_info = extract_note_info(&results, label_kilns);
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
    let requested = entries.len();
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

    // A handler that asked for notes and produced nothing usable is buggy, not
    // decisive: every entry was malformed, out of range or duplicated. Fall
    // back rather than dropping the agent's grounding on a typo. Only a
    // genuinely empty return means "suppress this turn".
    if requested > 0 && selected.is_empty() {
        warn!(
            requested,
            "precognition_select produced no usable entries; falling back to the default"
        );
        return None;
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
    label_kilns: bool,
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
                .filter(|_| label_kilns)
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
mod tests;
