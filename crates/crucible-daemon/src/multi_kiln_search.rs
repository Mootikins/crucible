use anyhow::Result;
use crucible_config::{DataClassification, TrustLevel};
use crucible_core::database::{DocumentId, SearchResult};
use crucible_core::traits::KnowledgeRepository;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::trust_resolution::resolve_kiln_classification;

pub struct KilnSearchSource {
    pub kiln_path: PathBuf,
    pub knowledge_repo: Arc<dyn KnowledgeRepository>,
    pub is_primary: bool,
}

pub async fn search_across_kilns(
    sources: &[KilnSearchSource],
    query_embedding: Vec<f32>,
    top_k: usize,
    provider_trust: Option<TrustLevel>,
    workspace: &Path,
) -> Result<Vec<SearchResult>> {
    let mut best: HashMap<(PathBuf, String), SearchResult> = HashMap::new();

    for source in sources {
        // Trust filtering: skip kilns whose classification exceeds provider trust
        if !source.is_primary {
            if let Some(trust) = provider_trust {
                let classification = resolve_kiln_classification(workspace, &source.kiln_path)
                    .unwrap_or(DataClassification::Public);
                if !trust.satisfies(classification) {
                    tracing::debug!(
                        "Skipping kiln {}: classification {} exceeds provider trust {}",
                        source.kiln_path.display(),
                        classification,
                        trust
                    );
                    continue;
                }
            }
        }
        let results = match source
            .knowledge_repo
            .search_vectors(query_embedding.clone())
            .await
        {
            Ok(results) => results,
            Err(e) => {
                tracing::warn!(
                    "Kiln search failed for {}: {}",
                    source.kiln_path.display(),
                    e
                );
                continue;
            }
        };

        for mut result in results {
            result.kiln_path = Some(source.kiln_path.clone());
            let doc_id: DocumentId = result.document_id.clone();
            let key = (source.kiln_path.clone(), doc_id.0.clone());

            best.entry(key)
                .and_modify(|existing| {
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert(result);
        }
    }

    let mut merged: Vec<SearchResult> = best.into_values().collect();
    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    merged.truncate(top_k);

    Ok(merged)
}
