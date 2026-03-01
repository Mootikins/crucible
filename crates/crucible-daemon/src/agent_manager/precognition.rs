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
                // TODO: this comparison is now trivially true since all kilns share one enrichment config; future work should compare stored model metadata
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

        let results = match self
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

        let note_info = extract_note_info(&results, &session.kiln);
        let enriched_prompt =
            Self::format_precognition_context(original_content, &results, &session.kiln);

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

/// Extract `PrecognitionNoteInfo` metadata from search results.
/// Titles are derived from the document ID filename (without `.md`).
/// `kiln_label` is set only for results from non-primary kilns.
pub(super) fn extract_note_info(
    results: &[crucible_core::SearchResult],
    primary_kiln: &std::path::Path,
) -> Vec<crucible_core::traits::chat::PrecognitionNoteInfo> {
    results
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
            let kiln_label = r
                .kiln_path
                .as_ref()
                .filter(|path| path.as_path() != primary_kiln)
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string());
            crucible_core::traits::chat::PrecognitionNoteInfo { title, kiln_label }
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
}
