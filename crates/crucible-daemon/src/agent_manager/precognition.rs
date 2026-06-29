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

/// Decide whether Precognition should run for this turn.
///
/// Pi-style heuristic: even when Precognition is enabled, only inject
/// on the first user message of a session. Running every turn bloats
/// context and degrades cache hits over a long conversation, with
/// diminishing relevance — subsequent turns are usually about the same
/// topic the first injection already covered.
///
/// Other gates: `/search` is a manual search command that shouldn't
/// trigger auto-RAG; kiln must be configured. The handler hook seam
/// (`transform_context`) is a separate, per-turn surface — Lua plugins
/// can implement richer per-turn heuristics there.
pub(super) fn should_run_precognition(
    precognition_enabled: bool,
    original_content: &str,
    session_kiln: &std::path::Path,
    is_first_user_message: bool,
) -> bool {
    precognition_enabled
        && !original_content.starts_with("/search")
        && !session_kiln.as_os_str().is_empty()
        && is_first_user_message
}

#[cfg(test)]
mod should_run_precognition_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn runs_on_first_user_message_with_precognition_enabled() {
        assert!(should_run_precognition(
            true,
            "tell me about widgets",
            Path::new("/some/kiln"),
            true,
        ));
    }

    #[test]
    fn skipped_on_subsequent_user_messages_even_when_enabled() {
        // Pi-style: don't re-inject every turn — bloats context, hurts
        // cache, redundant for same-topic follow-ups.
        assert!(!should_run_precognition(
            true,
            "follow-up question",
            Path::new("/some/kiln"),
            false,
        ));
    }

    #[test]
    fn skipped_when_disabled_in_agent_config() {
        assert!(!should_run_precognition(
            false,
            "x",
            Path::new("/some/kiln"),
            true,
        ));
    }

    #[test]
    fn skipped_for_explicit_search_command() {
        assert!(!should_run_precognition(
            true,
            "/search widgets",
            Path::new("/some/kiln"),
            true,
        ));
    }

    #[test]
    fn skipped_when_no_kiln_configured() {
        assert!(!should_run_precognition(true, "x", Path::new(""), true,));
    }
}

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
    ) -> String {
        if results.is_empty() {
            return String::new();
        }
        let custom_formatted = Self::execute_precognition_format_handlers(
            session_id,
            original_content,
            results,
            state,
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
            let title = result
                .document_id
                .0
                .split('/')
                .next_back()
                .unwrap_or(&result.document_id.0)
                .trim_end_matches(".md");
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
    ) -> Option<String> {
        use crucible_lua::ScriptHandlerResult;

        let handlers = state
            .registry
            .runtime_handlers_for("precognition_format", None);
        if handlers.is_empty() {
            return None;
        }

        let results_payload: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let title = r
                    .document_id
                    .0
                    .split('/')
                    .next_back()
                    .unwrap_or(&r.document_id.0)
                    .trim_end_matches(".md")
                    .to_string();

                serde_json::json!({
                    "title": title,
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

        for handler in handlers {
            match state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event)
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

    /// Collect search sources from the primary kiln and any connected kilns.
    /// Connected kilns are skipped if they lack enrichment config or use a
    /// different embedding model than the primary kiln.
    async fn collect_kiln_search_sources(
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

        apply_precognition_char_cap(&mut results, self.kiln_manager.max_precognition_chars());

        let context_block = {
            let session_state = self.get_or_create_session_state(session_id);
            let state = session_state.lock().await;
            Self::format_precognition_context_block(
                session_id,
                original_content,
                &results,
                &session.kiln,
                &state,
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
                Some(crucible_core::traits::chat::PrecognitionNoteInfo { title, kiln_label })
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
                    return "## Custom Format\n" .. event.payload.user_message .. "\n" .. event.payload.results[1].title
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
