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
    fn format_with_precognition_runtime_hook(
        session_id: &str,
        original_content: &str,
        results: &[crucible_core::SearchResult],
        primary_kiln: &std::path::Path,
        state: &SessionEventState,
    ) -> String {
        let custom_formatted = Self::execute_precognition_format_handlers(
            session_id,
            original_content,
            results,
            state,
        );

        if let Some(context_block) = custom_formatted {
            format!("{}\n\n{}", context_block, original_content)
        } else {
            Self::format_precognition_context(original_content, results, primary_kiln)
        }
    }

    fn execute_precognition_format_handlers(
        session_id: &str,
        original_content: &str,
        results: &[crucible_core::SearchResult],
        state: &SessionEventState,
    ) -> Option<String> {
        use crucible_lua::ScriptHandlerResult;

        let handlers = state.registry.runtime_handlers_for("precognition_format");
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
            {
                Ok(ScriptHandlerResult::Transform(value)) => {
                    if let Some(formatted) = value.as_str() {
                        return Some(formatted.to_string());
                    }
                }
                Ok(ScriptHandlerResult::PassThrough)
                | Ok(ScriptHandlerResult::Cancel { .. })
                | Ok(ScriptHandlerResult::Inject { .. }) => {}
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
        primary_config: &crucible_config::EmbeddingProviderConfig,
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
            5,
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

    /// Format search results into a precognition-enriched prompt.
    /// If no results are found the original content is returned unchanged.
    fn format_precognition_context(
        original_content: &str,
        results: &[crucible_core::SearchResult],
        primary_kiln: &std::path::Path,
    ) -> String {
        if results.is_empty() {
            return original_content.to_string();
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

        format!("{}\n</system>\n\n{}", context, original_content)
    }

    /// Returns the original content unchanged on any failure.
    pub(super) async fn enrich_with_precognition(
        &self,
        session_id: &str,
        original_content: &str,
        session: &crucible_core::session::Session,
        agent_config: &SessionAgent,
        event_tx: &broadcast::Sender<SessionEventMessage>,
    ) -> String {
        let kiln_path = session.kiln.as_path();

        let handle = match self.kiln_manager.get_or_open(kiln_path).await {
            Ok(h) => h,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Failed to open kiln for precognition");
                return original_content.to_string();
            }
        };

        let primary_config = match self.kiln_manager.enrichment_config().cloned() {
            Some(c) => c,
            None => return original_content.to_string(),
        };

        let embedding_provider = match crate::embedding::get_or_create_embedding_provider(
            &primary_config,
        )
        .await
        {
            Ok(p) => p,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Failed to create embedding provider for precognition");
                return original_content.to_string();
            }
        };

        let query_embedding = match embedding_provider.embed(original_content).await {
            Ok(e) => e,
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Precognition embedding failed");
                emit_precognition_event(event_tx, session_id, original_content, 0, 1, 1, None);
                return original_content.to_string();
            }
        };

        let sources = self
            .collect_kiln_search_sources(session_id, session, &handle, &primary_config)
            .await;
        let kilns_searched = sources.len();

        let mut results = match self
            .execute_multi_kiln_search(ExecuteMultiKilnSearchParams {
                session_id,
                sources: &sources,
                query_embedding,
                agent_config,
                session,
                event_tx,
                original_content,
            })
            .await
        {
            Some(r) => r,
            None => return original_content.to_string(),
        };

        apply_precognition_char_cap(&mut results, self.kiln_manager.max_precognition_chars());

        let enriched_prompt = {
            let session_state = self.get_or_create_session_state(session_id);
            let state = session_state.lock().await;
            Self::format_with_precognition_runtime_hook(
                session_id,
                original_content,
                &results,
                &session.kiln,
                &state,
            )
        };
        let note_info = extract_note_info(&results, &session.kiln);

        emit_precognition_event(
            event_tx,
            session_id,
            original_content,
            results.len(),
            kilns_searched,
            0,
            Some(note_info),
        );
        enriched_prompt
    }
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
pub(super) fn extract_note_info(
    results: &[crucible_core::SearchResult],
    primary_kiln: &std::path::Path,
) -> Vec<crucible_core::traits::chat::PrecognitionNoteInfo> {
    let mut seen = std::collections::HashSet::new();
    results
        .iter()
        .filter_map(|r| {
            let title = r
                .document_id
                .0
                .split('/')
                .next_back()
                .unwrap_or(&r.document_id.0)
                .trim_end_matches(".md")
                .to_string();
            let kiln_label = r
                .kiln_path
                .as_ref()
                .filter(|path| path.as_path() != primary_kiln)
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string());
            // Deduplicate by (title, kiln_label) — same note with multiple
            // embeddings or blocks should appear only once.
            if seen.insert((title.clone(), kiln_label.clone())) {
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

    #[test]
    fn format_precognition_context_empty_results_returns_original() {
        let result = AgentManager::format_precognition_context(
            "What is Rust?",
            &[],
            std::path::Path::new("/home/user/notes"),
        );
        assert_eq!(result, "What is Rust?");
    }

    #[test]
    fn format_precognition_context_single_result_has_system_tags() {
        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::format_precognition_context(
            "What is Rust?",
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(
            output.starts_with("<system>\n"),
            "Should start with <system> tag"
        );
        assert!(
            output.contains("</system>"),
            "Should contain closing </system> tag"
        );
        assert!(
            output.contains("Found 1 relevant notes:"),
            "Should state result count"
        );
        assert!(
            output.contains("## Rust"),
            "Should contain note title without .md"
        );
        assert!(
            output.contains("(similarity: 0.85)"),
            "Should contain similarity score"
        );
        assert!(
            output.contains("Rust is a systems programming language."),
            "Should contain snippet"
        );
        assert!(
            output.ends_with("What is Rust?"),
            "Original content should follow after </system>"
        );
    }

    #[test]
    fn format_precognition_context_multiple_results() {
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

        let output = AgentManager::format_precognition_context(
            "Compare languages",
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(output.contains("Found 2 relevant notes:"));
        assert!(output.contains("## Rust"));
        assert!(output.contains("## Go"));
        assert!(output.contains("Rust is fast."));
        assert!(output.contains("Go is simple."));
        assert!(output.ends_with("Compare languages"));
    }

    #[test]
    fn format_precognition_context_kiln_label_for_non_primary() {
        let results = vec![make_result(
            "notes/External.md",
            0.70,
            Some("External content."),
            Some("/other/kiln"),
        )];

        let output = AgentManager::format_precognition_context(
            "query",
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(
            output.contains("[from: kiln]"),
            "Non-primary kiln should have label"
        );
    }

    #[test]
    fn format_precognition_context_no_kiln_label_for_primary() {
        let results = vec![make_result(
            "notes/Local.md",
            0.90,
            Some("Local content."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::format_precognition_context(
            "query",
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        assert!(
            !output.contains("[from:"),
            "Primary kiln should not have label"
        );
    }

    #[test]
    fn format_precognition_context_missing_snippet_handled() {
        let results = vec![make_result(
            "notes/NoSnippet.md",
            0.60,
            None,
            Some("/home/user/notes"),
        )];

        let output = AgentManager::format_precognition_context(
            "query",
            &results,
            std::path::Path::new("/home/user/notes"),
        );

        // Should not panic; empty string used for missing snippet
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
        }
    }

    #[test]
    fn precognition_format_hook_customizes_output() {
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

        let output = AgentManager::format_with_precognition_runtime_hook(
            "session-1",
            "What is Rust?",
            &results,
            std::path::Path::new("/home/user/notes"),
            &state,
        );

        assert!(output.starts_with("## Custom Format"));
        assert!(output.contains("What is Rust?"));
        assert!(output.contains("Rust"));
        assert!(!output.starts_with("<system>"));
    }

    #[test]
    fn precognition_format_no_handler_uses_default() {
        let state = make_session_event_state();
        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::format_with_precognition_runtime_hook(
            "session-1",
            "What is Rust?",
            &results,
            std::path::Path::new("/home/user/notes"),
            &state,
        );

        assert!(output.contains("<system>"));
        assert!(output.contains("Found 1 relevant notes:"));
        assert!(output.ends_with("What is Rust?"));
    }

    #[test]
    fn extract_note_info_deduplicates_same_title() {
        // Same note with multiple embeddings (block-level) should appear once
        let results = vec![
            make_result(
                "notes/Getting Started.md",
                0.9,
                Some("block 1"),
                Some("/kiln"),
            ),
            make_result(
                "notes/Getting Started.md",
                0.8,
                Some("block 2"),
                Some("/kiln"),
            ),
            make_result("notes/Plugins.md", 0.7, Some("plugin info"), Some("/kiln")),
        ];

        let info = extract_note_info(&results, std::path::Path::new("/kiln"));
        let titles: Vec<&str> = info.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["Getting Started", "Plugins"]);
    }

    #[test]
    fn extract_note_info_keeps_different_kiln_labels() {
        // Same title from different kilns are kept as separate entries
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
