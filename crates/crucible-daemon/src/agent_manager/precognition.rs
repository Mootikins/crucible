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

            let Some(connected_config) = self
                .kiln_manager
                .enrichment_config()
                .cloned()
            else {
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
    #[allow(clippy::too_many_arguments)]
    async fn execute_multi_kiln_search(
        &self,
        session_id: &str,
        sources: &[KilnSearchSource],
        query_embedding: Vec<f32>,
        agent_config: &SessionAgent,
        session: &crucible_core::session::Session,
        event_tx: &broadcast::Sender<SessionEventMessage>,
        original_content: &str,
    ) -> Option<Vec<crucible_core::SearchResult>> {
        let provider_trust = resolve_provider_trust(agent_config, self.llm_config.as_ref());
        let kilns_searched = sources.len();

        match search_across_kilns(
            sources,
            query_embedding,
            5,
            Some(provider_trust),
            &session.workspace,
        )
        .await
        {
            Ok(r) => Some(r),
            Err(error) => {
                warn!(session_id = %session_id, error = %error, "Precognition search across kilns failed");
                emit_precognition_event(
                    event_tx,
                    session_id,
                    original_content,
                    0,
                    kilns_searched,
                    1,
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

        let context = results
            .iter()
            .enumerate()
            .map(|(i, result)| {
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

                format!(
                    "## Context #{}: {}{} (similarity: {:.2})\n\n{}\n",
                    i + 1,
                    title,
                    kiln_label,
                    result.score,
                    result.snippet.clone().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
            context, original_content
        )
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
                emit_precognition_event(event_tx, session_id, original_content, 0, 1, 1);
                return original_content.to_string();
            }
        };

        let sources = self
            .collect_kiln_search_sources(session_id, session, &handle, &primary_config)
            .await;
        let kilns_searched = sources.len();

        let results = match self
            .execute_multi_kiln_search(
                session_id,
                &sources,
                query_embedding,
                agent_config,
                session,
                event_tx,
                original_content,
            )
            .await
        {
            Some(r) => r,
            None => return original_content.to_string(),
        };

        let enriched_prompt =
            Self::format_precognition_context(original_content, &results, &session.kiln);

        emit_precognition_event(
            event_tx,
            session_id,
            original_content,
            results.len(),
            kilns_searched,
            0,
        );
        enriched_prompt
    }
}
