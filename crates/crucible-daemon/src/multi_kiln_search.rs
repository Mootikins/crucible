use crate::retrieval_stage::{first_usable_transform, has_handlers, lua_array, StageVm};
use crate::trust_resolution::resolve_session_classification;
use anyhow::Result;
use crucible_core::config::{DataClassification, TrustLevel};
use crucible_core::events::SessionEvent;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::{DocumentId, SearchResult};
use crucible_lua::StageId;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One corpus the fan-out searches, and the only place a kiln's directory and
/// its registry name sit side by side.
///
/// The path is for *reaching* the corpus — trust resolution, dedup keys, log
/// lines the machine owner reads. The name is for *reporting* it, and it is the
/// only half that is copied onto a [`SearchResult`]. Building a source is
/// therefore the one point where a caller has to answer "what is this kiln
/// called", and `None` is a legitimate answer (an unregistered kiln opened by
/// path, e.g. `cru mcp`) that means every downstream renderer omits the field.
#[derive(Clone)]
pub struct KilnSearchSource {
    pub kiln_path: PathBuf,
    /// The registry name, when this source came from a registered kiln.
    pub kiln_name: Option<crucible_core::config::KilnName>,
    pub knowledge_repo: Arc<dyn KnowledgeRepository>,
}

/// The `search:rerank` stage a search fires after the merge and before the
/// cut. The VMs run in order, first usable Transform wins: precognition lists
/// the session VM before the plugin VM; the tool and the RPC list the plugin
/// VM alone.
#[derive(Clone)]
pub struct RerankStage {
    /// The session the handlers run for, when the caller has one.
    pub session_id: Option<String>,
    /// Session VM first, then the plugin VM.
    pub vms: Vec<StageVm>,
    /// Each source answers `top_k * fanout` rows when a handler is
    /// registered, so a rerank has more than the final cut to choose from.
    pub fanout: usize,
}

impl RerankStage {
    /// A stage over `vms` with no over-fetch.
    pub fn new(session_id: Option<String>, vms: Vec<StageVm>) -> Self {
        Self {
            session_id,
            vms,
            fanout: 1,
        }
    }
}

/// [`search_across_kilns_with_stage`] with no rerank stage.
pub async fn search_across_kilns(
    sources: &[KilnSearchSource],
    query_embedding: Vec<f32>,
    top_k: usize,
    provider_trust: Option<TrustLevel>,
    workspace: Option<&Path>,
) -> Result<Vec<SearchResult>> {
    search_across_kilns_with_stage(
        sources,
        query_embedding,
        top_k,
        provider_trust,
        workspace,
        None,
    )
    .await
}

pub async fn search_across_kilns_with_stage(
    sources: &[KilnSearchSource],
    query_embedding: Vec<f32>,
    top_k: usize,
    provider_trust: Option<TrustLevel>,
    workspace: Option<&Path>,
    rerank: Option<&RerankStage>,
) -> Result<Vec<SearchResult>> {
    let mut best: HashMap<(PathBuf, String, Option<usize>), SearchResult> = HashMap::new();

    // Over-fetch only when a handler will look at the extra rows.
    let rerank = rerank.filter(|stage| has_handlers(StageId::SearchRerank, &stage.vms));
    let fetch = rerank.map_or(top_k, |stage| top_k.saturating_mul(stage.fanout.max(1)));

    for source in sources {
        // Trust filtering: skip kilns whose classification exceeds provider
        // trust. Every source is filtered — the session's kiln set is flat, so
        // there is no member that gets to skip the check by being first.
        if let Some(trust) = provider_trust {
            // `.unwrap_or(Public)` is the permit here, so the lookup must not
            // be able to come up empty merely because the session has no
            // workspace to read a config from: the live-session resolver walks
            // up from the kiln when the workspace answers nothing.
            let classification = resolve_session_classification(workspace, &source.kiln_path)
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
        // Blocks first: a hit that names a passage is strictly more useful
        // than one that names the file it sat in. A kiln indexed before the
        // block store existed answers nothing here, so the note search stays
        // as the fallback rather than as a second-class path.
        let block_results = match source
            .knowledge_repo
            .search_blocks(query_embedding.clone(), fetch)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                tracing::warn!(
                    "Kiln block search failed for {}, falling back to notes: {}",
                    source.kiln_path.display(),
                    e
                );
                Vec::new()
            }
        };

        let results = if block_results.is_empty() {
            match source
                .knowledge_repo
                .search_vectors(query_embedding.clone(), fetch)
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
            }
        } else {
            block_results
        };

        for mut result in results {
            // The name, not the path. The dedup key below still uses the path —
            // it is the identity of the corpus, and two kilns may share a name
            // in a malformed config — but nothing that leaves this function
            // carries one.
            result.kiln = source.kiln_name.clone();
            let doc_id: DocumentId = result.document_id.clone();
            // Several blocks of one note are several hits, so the span joins
            // the key. Without it the merge would keep one block per note and
            // throw the granularity away at the last step.
            let key = (
                source.kiln_path.clone(),
                doc_id.0.clone(),
                result.block.as_ref().map(|b| b.span_start),
            );

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

    if let Some(stage) = rerank {
        let event = rerank_event(sources, &query_embedding, top_k, &merged);
        let reranked = first_usable_transform(
            StageId::SearchRerank,
            &stage.vms,
            stage.session_id.as_deref(),
            &event,
            |value| apply_rerank(&merged, value),
        )
        .await;
        if let Some(reranked) = reranked {
            merged = reranked;
        }
    }
    merged.truncate(top_k);

    Ok(merged)
}

/// The `search:rerank` event: the query, the kilns, the limit, and every
/// merged hit with a 1-based `index` the handler returns to address it.
fn rerank_event(
    sources: &[KilnSearchSource],
    query_embedding: &[f32],
    top_k: usize,
    hits: &[SearchResult],
) -> SessionEvent {
    let kilns: Vec<&str> = sources
        .iter()
        .filter_map(|s| s.kiln_name.as_ref())
        .map(|n| n.as_str())
        .collect();
    let hits: Vec<serde_json::Value> = hits
        .iter()
        .enumerate()
        .map(|(position, hit)| {
            let mut entry = serde_json::Map::new();
            entry.insert("index".into(), serde_json::json!(position + 1));
            if let Some(name) = hit.kiln.as_ref() {
                entry.insert("kiln".into(), serde_json::json!(name.as_str()));
            }
            entry.insert("path".into(), serde_json::json!(hit.document_id.0));
            entry.insert("score".into(), serde_json::json!(hit.score));
            if let Some(snippet) = hit.snippet.as_ref() {
                entry.insert("snippet".into(), serde_json::json!(snippet));
            }
            if let Some(block) = hit.block.as_ref() {
                entry.insert("span_start".into(), serde_json::json!(block.span_start));
                entry.insert("span_end".into(), serde_json::json!(block.span_end));
                entry.insert("kind".into(), serde_json::json!(block.kind));
            }
            serde_json::Value::Object(entry)
        })
        .collect();
    SessionEvent::Custom {
        name: StageId::SearchRerank.as_str().to_string(),
        payload: serde_json::json!({
            "query_vector": query_embedding,
            "kilns": kilns,
            "limit": top_k,
            "hits": hits,
        }),
    }
}

/// Read a `search:rerank` Transform over the merged hits.
///
/// Entries address hits by `index`, so a handler can reorder, rescore, widen
/// and cite but never introduce a hit the kilns did not return. `None` means
/// the value is not a list, or every entry was unusable; the merged order
/// then stands. An empty list is a decision and yields no hits.
fn apply_rerank(merged: &[SearchResult], value: &serde_json::Value) -> Option<Vec<SearchResult>> {
    let entries = lua_array(value)?;
    let mut seen = std::collections::HashSet::new();
    let mut reranked = Vec::with_capacity(entries.len());

    for entry in &entries {
        let Some(index) = entry.get("index").and_then(|v| v.as_u64()) else {
            tracing::warn!("search:rerank entry missing numeric `index`; dropping");
            continue;
        };
        let Some(original) = index
            .checked_sub(1)
            .and_then(|zero_based| merged.get(zero_based as usize))
        else {
            tracing::warn!(index, "search:rerank index out of range; dropping");
            continue;
        };
        if !seen.insert(index) {
            tracing::warn!(index, "search:rerank duplicate index; dropping");
            continue;
        }

        let mut hit = original.clone();
        if let Some(score) = entry.get("score").and_then(|v| v.as_f64()) {
            hit.score = score;
        }
        if let Some(block) = hit.block.as_mut() {
            if let Some(span_end) = entry.get("span_end").and_then(|v| v.as_u64()) {
                let span_end = span_end as usize;
                if span_end >= block.span_start {
                    block.span_end = span_end;
                }
            }
            if let Some(cited) = entry.get("cited").and_then(lua_array) {
                block.cited = cited
                    .iter()
                    .filter_map(|pair| {
                        let pair = lua_array(pair)?;
                        let start = pair.first()?.as_u64()? as usize;
                        let stop = pair.get(1)?.as_u64()? as usize;
                        (start <= stop).then_some((start, stop))
                    })
                    .collect();
            }
        }
        reranked.push(hit);
    }

    if !entries.is_empty() && reranked.is_empty() {
        tracing::warn!(
            requested = entries.len(),
            "search:rerank handler returned no usable entry; keeping the merged order"
        );
        return None;
    }
    Some(reranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockKnowledgeRepository;
    use std::fs;
    use tempfile::TempDir;

    fn mock_result(document_id: &str, score: f64) -> SearchResult {
        SearchResult {
            document_id: DocumentId(document_id.to_string()),
            score,
            highlights: None,
            snippet: None,
            kiln: None,
            block: None,
        }
    }

    /// The registry name a fixture kiln directory stands in for. Production
    /// takes this from the session's own `kilns` list, never from the path;
    /// folding the basename here just spares every test a literal.
    fn name_of(kiln_path: &Path) -> Option<crucible_core::config::KilnName> {
        kiln_path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(crucible_core::config::KilnName::normalize)
    }

    fn mock_source(
        kiln_path: PathBuf,
        results: Vec<SearchResult>,
        should_fail: bool,
    ) -> KilnSearchSource {
        KilnSearchSource {
            kiln_name: name_of(&kiln_path),
            kiln_path,
            knowledge_repo: Arc::new(if should_fail {
                MockKnowledgeRepository::failing()
            } else {
                MockKnowledgeRepository::with_results(results)
            }),
        }
    }

    /// An unnamed source (a kiln opened by path, outside the registry) must
    /// leave `kiln` absent. There is no fallback to the directory basename:
    /// that is the disclosure this field was retyped to make unrepresentable.
    fn unnamed_source(
        kiln_path: PathBuf,
        results: Vec<SearchResult>,
        should_fail: bool,
    ) -> KilnSearchSource {
        KilnSearchSource {
            kiln_path,
            kiln_name: None,
            knowledge_repo: Arc::new(if should_fail {
                MockKnowledgeRepository::failing()
            } else {
                MockKnowledgeRepository::with_results(results)
            }),
        }
    }

    fn write_workspace_config(workspace: &Path, kilns: &[(&str, Option<&str>)]) {
        let crucible_dir = workspace.join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();

        let mut toml = String::from("[workspace]\nname = \"test\"\n");
        for (path, classification) in kilns {
            toml.push_str("\n[[kilns]]\n");
            toml.push_str(&format!("path = \"{path}\"\n"));
            if let Some(value) = classification {
                toml.push_str(&format!("data_classification = \"{value}\"\n"));
            }
        }

        fs::write(crucible_dir.join("project.toml"), toml).unwrap();
    }

    #[tokio::test]
    async fn search_empty_sources_returns_empty() {
        let tmp = TempDir::new().unwrap();

        let results = search_across_kilns(&[], vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_single_kiln_returns_results() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("notes");
        fs::create_dir_all(&kiln).unwrap();

        let sources = vec![mock_source(
            kiln.clone(),
            vec![mock_result("doc1", 0.8), mock_result("doc2", 0.4)],
            false,
        )];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.kiln == name_of(&kiln)));
    }

    #[tokio::test]
    async fn search_two_kilns_merges_and_sorts() {
        let tmp = TempDir::new().unwrap();
        let kiln_a = tmp.path().join("kiln-a");
        let kiln_b = tmp.path().join("kiln-b");
        fs::create_dir_all(&kiln_a).unwrap();
        fs::create_dir_all(&kiln_b).unwrap();

        let sources = vec![
            mock_source(
                kiln_a,
                vec![mock_result("a-1", 0.2), mock_result("a-2", 0.9)],
                false,
            ),
            mock_source(
                kiln_b,
                vec![mock_result("b-1", 0.6), mock_result("b-2", 0.3)],
                false,
            ),
        ];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(results.len(), 4);
        assert_eq!(results[0].document_id.0, "a-2");
        assert_eq!(results[1].document_id.0, "b-1");
        assert_eq!(results[2].document_id.0, "b-2");
        assert_eq!(results[3].document_id.0, "a-1");
    }

    #[tokio::test]
    async fn search_dedup_same_document_keeps_highest_score() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("notes");
        fs::create_dir_all(&kiln).unwrap();

        let sources = vec![mock_source(
            kiln,
            vec![mock_result("doc1", 0.3), mock_result("doc1", 0.95)],
            false,
        )];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id.0, "doc1");
        assert_eq!(results[0].score, 0.95);
    }

    #[tokio::test]
    async fn search_one_kiln_fails_other_succeeds() {
        let tmp = TempDir::new().unwrap();
        let good = tmp.path().join("good");
        let bad = tmp.path().join("bad");
        fs::create_dir_all(&good).unwrap();
        fs::create_dir_all(&bad).unwrap();

        let sources = vec![
            mock_source(bad, vec![mock_result("bad-doc", 0.9)], true),
            mock_source(good.clone(), vec![mock_result("good-doc", 0.7)], false),
        ];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id.0, "good-doc");
        assert_eq!(results[0].kiln, name_of(&good));
    }

    /// The whole point of the retype: a source with no registry name yields
    /// hits with no kiln at all, rather than hits labelled with the directory
    /// the daemon happened to open. Every renderer downstream — the precognition
    /// block, the `semantic_search` tool result, the Lua payload, the transcript
    /// — reads this one field, so an absent name is absent everywhere.
    #[tokio::test]
    async fn unnamed_kiln_leaves_the_result_unattributed() {
        let tmp = TempDir::new().unwrap();
        let kiln = tmp.path().join("Private Vault");
        fs::create_dir_all(&kiln).unwrap();

        let sources = vec![unnamed_source(
            kiln.clone(),
            vec![mock_result("secret.md", 0.9)],
            false,
        )];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].kiln, None,
            "an unregistered kiln must not be named after its directory"
        );
        let rendered = serde_json::to_string(&results[0]).unwrap();
        assert!(
            !rendered.contains("Private Vault"),
            "no serialization of a hit may carry the kiln's directory: {rendered}"
        );
    }

    #[tokio::test]
    async fn search_kiln_name_populated_on_results() {
        let tmp = TempDir::new().unwrap();
        let kiln_a = tmp.path().join("kiln-a");
        let kiln_b = tmp.path().join("kiln-b");
        fs::create_dir_all(&kiln_a).unwrap();
        fs::create_dir_all(&kiln_b).unwrap();

        let sources = vec![
            mock_source(kiln_a.clone(), vec![mock_result("doc-a", 0.6)], false),
            mock_source(kiln_b.clone(), vec![mock_result("doc-b", 0.5)], false),
        ];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(tmp.path()))
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|r| { r.document_id.0 == "doc-a" && r.kiln == name_of(&kiln_a) }));
        assert!(results
            .iter()
            .any(|r| { r.document_id.0 == "doc-b" && r.kiln == name_of(&kiln_b) }));
    }

    #[tokio::test]
    async fn trust_filter_skips_confidential_with_cloud_trust() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let primary = workspace.join("primary");
        let confidential = workspace.join("confidential");
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&confidential).unwrap();

        write_workspace_config(
            &workspace,
            &[
                ("./primary", Some("public")),
                ("./confidential", Some("confidential")),
            ],
        );

        let sources = vec![
            mock_source(
                primary.clone(),
                vec![mock_result("primary-doc", 0.5)],
                false,
            ),
            mock_source(
                confidential,
                vec![mock_result("confidential-doc", 0.99)],
                false,
            ),
        ];

        let results = search_across_kilns(
            &sources,
            vec![0.1, 0.2],
            10,
            Some(TrustLevel::Cloud),
            Some(&workspace),
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id.0, "primary-doc");
        assert_eq!(results[0].kiln, name_of(&primary));
    }

    #[tokio::test]
    async fn trust_filter_allows_public_with_any_trust() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let public_kiln = workspace.join("public");
        fs::create_dir_all(&public_kiln).unwrap();

        write_workspace_config(&workspace, &[("./public", Some("public"))]);

        let sources = vec![mock_source(
            public_kiln.clone(),
            vec![mock_result("public-doc", 0.77)],
            false,
        )];

        let results = search_across_kilns(
            &sources,
            vec![0.1, 0.2],
            10,
            Some(TrustLevel::Untrusted),
            Some(&workspace),
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id.0, "public-doc");
        assert_eq!(results[0].kiln, name_of(&public_kiln));
    }

    #[tokio::test]
    async fn trust_filter_none_provider_trust_searches_all() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let primary = workspace.join("primary");
        let confidential = workspace.join("confidential");
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&confidential).unwrap();

        write_workspace_config(
            &workspace,
            &[
                ("./primary", Some("public")),
                ("./confidential", Some("confidential")),
            ],
        );

        let sources = vec![
            mock_source(primary, vec![mock_result("primary-doc", 0.5)], false),
            mock_source(
                confidential,
                vec![mock_result("confidential-doc", 0.9)],
                false,
            ),
        ];

        let results = search_across_kilns(&sources, vec![0.1, 0.2], 10, None, Some(&workspace))
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.document_id.0 == "primary-doc"));
        assert!(results
            .iter()
            .any(|r| r.document_id.0 == "confidential-doc"));
    }

    #[tokio::test]
    async fn trust_filter_unclassified_defaults_to_public() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let primary = workspace.join("primary");
        let unclassified = workspace.join("unclassified");
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&unclassified).unwrap();

        write_workspace_config(&workspace, &[("./primary", Some("public"))]);

        let sources = vec![
            mock_source(primary, vec![mock_result("primary-doc", 0.4)], false),
            mock_source(
                unclassified,
                vec![mock_result("unclassified-doc", 0.8)],
                false,
            ),
        ];

        let results = search_across_kilns(
            &sources,
            vec![0.1, 0.2],
            10,
            Some(TrustLevel::Cloud),
            Some(&workspace),
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|r| r.document_id.0 == "unclassified-doc"));
    }

    fn block_result(document_id: &str, score: f64, span_start: usize) -> SearchResult {
        SearchResult {
            document_id: DocumentId(document_id.to_string()),
            score,
            highlights: None,
            snippet: Some(format!("passage at {span_start}")),
            kiln: None,
            block: Some(crucible_core::types::database::BlockRef {
                span_start,
                span_end: span_start + 10,
                kind: "paragraph".to_string(),
                cited: Vec::new(),
            }),
        }
    }

    #[tokio::test]
    async fn block_hits_win_over_whole_note_hits() {
        let dir = TempDir::new().unwrap();
        let repo = MockKnowledgeRepository::with_results(vec![mock_result("a.md", 0.99)])
            .with_block_results(vec![block_result("a.md", 0.5, 40)]);
        let sources = vec![KilnSearchSource {
            kiln_path: dir.path().to_path_buf(),
            kiln_name: None,
            knowledge_repo: Arc::new(repo),
        }];

        let results = search_across_kilns(&sources, vec![1.0], 10, None, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            results[0].block.is_some(),
            "a kiln with block rows must answer with passages, even at a lower score"
        );
    }

    #[tokio::test]
    async fn a_kiln_without_block_rows_falls_back_to_notes() {
        let dir = TempDir::new().unwrap();
        // No block results scripted: what an un-reindexed kiln looks like.
        let repo = MockKnowledgeRepository::with_results(vec![mock_result("a.md", 0.7)]);
        let sources = vec![KilnSearchSource {
            kiln_path: dir.path().to_path_buf(),
            kiln_name: None,
            knowledge_repo: Arc::new(repo),
        }];

        let results = search_across_kilns(&sources, vec![1.0], 10, None, None)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].block.is_none());
    }

    #[tokio::test]
    async fn several_blocks_of_one_note_all_survive_the_merge() {
        let dir = TempDir::new().unwrap();
        let repo = MockKnowledgeRepository::new().with_block_results(vec![
            block_result("a.md", 0.9, 0),
            block_result("a.md", 0.8, 40),
            block_result("a.md", 0.7, 80),
        ]);
        let sources = vec![KilnSearchSource {
            kiln_path: dir.path().to_path_buf(),
            kiln_name: None,
            knowledge_repo: Arc::new(repo),
        }];

        let results = search_across_kilns(&sources, vec![1.0], 10, None, None)
            .await
            .unwrap();

        // The dedup key is (kiln, note, span). Keyed on the note alone, this
        // would collapse to one and throw the granularity away at the last step.
        assert_eq!(results.len(), 3);
    }
}

/// The `search:rerank` stage, fired through a plugin VM with no session.
#[cfg(test)]
mod rerank_tests {
    use super::*;
    use crate::test_support::MockKnowledgeRepository;
    use crucible_lua::register_cru_on_api;
    use tempfile::TempDir;

    fn plugin_vm(handler: &str) -> StageVm {
        let lua = mlua::Lua::new();
        let registry = crucible_lua::LuaScriptHandlerRegistry::new();
        register_cru_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register_cru_on_api should succeed");
        lua.load(handler).exec().expect("the handler loads");
        (registry, lua)
    }

    fn block_hit(document_id: &str, score: f64, span_start: usize) -> SearchResult {
        SearchResult {
            document_id: DocumentId(document_id.to_string()),
            score,
            highlights: None,
            snippet: None,
            kiln: None,
            block: Some(crucible_core::types::database::BlockRef {
                span_start,
                span_end: span_start + 10,
                kind: "paragraph".to_string(),
                cited: Vec::new(),
            }),
        }
    }

    fn source(dir: &TempDir, hits: Vec<SearchResult>) -> Vec<KilnSearchSource> {
        vec![KilnSearchSource {
            kiln_path: dir.path().to_path_buf(),
            kiln_name: crucible_core::config::KilnName::normalize("lab"),
            knowledge_repo: Arc::new(MockKnowledgeRepository::new().with_block_results(hits)),
        }]
    }

    async fn search(
        sources: &[KilnSearchSource],
        top_k: usize,
        stage: &RerankStage,
    ) -> Vec<SearchResult> {
        search_across_kilns_with_stage(sources, vec![1.0, 0.0], top_k, None, None, Some(stage))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_handler_reorders_rescores_widens_and_cites() {
        let dir = TempDir::new().unwrap();
        let sources = source(
            &dir,
            vec![
                block_hit("a.md", 0.9, 0),
                block_hit("a.md", 0.8, 40),
                block_hit("b.md", 0.7, 0),
            ],
        );
        let stage = RerankStage::new(
            None,
            vec![plugin_vm(
                r#"
                cru.on("search:rerank", function(ctx, event)
                    assert(#event.query_vector == 2, "the query vector is a plain array")
                    assert(event.kilns[1] == "lab", "the kiln is named")
                    assert(event.limit == 10)
                    local out = {}
                    for i = #event.hits, 1, -1 do
                        local hit = event.hits[i]
                        assert(hit.path and hit.span_start and hit.kind == "paragraph")
                        out[#out + 1] = {
                            index = hit.index,
                            score = hit.score + 1,
                            span_end = hit.span_end + 30,
                            cited = { { hit.span_start, hit.span_end }, { 100, 120 } },
                        }
                    end
                    return out
                end)
                "#,
            )],
        );

        let results = search(&sources, 10, &stage).await;

        let order: Vec<(String, usize)> = results
            .iter()
            .map(|r| {
                (
                    r.document_id.0.clone(),
                    r.block.as_ref().unwrap().span_start,
                )
            })
            .collect();
        assert_eq!(
            order,
            vec![("b.md".into(), 0), ("a.md".into(), 40), ("a.md".into(), 0)]
        );
        assert!(
            (results[0].score - 1.7).abs() < 1e-9,
            "the score is replaced"
        );
        let block = results[0].block.as_ref().unwrap();
        assert_eq!(block.span_end, 40, "the span is widened");
        assert_eq!(block.cited, vec![(0, 10), (100, 120)]);
        assert_eq!(
            results[0].kiln,
            crucible_core::config::KilnName::normalize("lab")
        );
    }

    #[tokio::test]
    async fn the_cut_happens_after_the_rerank() {
        let dir = TempDir::new().unwrap();
        let sources = source(
            &dir,
            vec![block_hit("a.md", 0.9, 0), block_hit("b.md", 0.5, 0)],
        );
        let stage = RerankStage::new(
            None,
            vec![plugin_vm(
                r#"
                cru.on("search:rerank", function(ctx, event)
                    return { { index = 2 }, { index = 1 } }
                end)
                "#,
            )],
        );

        let results = search(&sources, 1, &stage).await;

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].document_id.0, "b.md",
            "the handler's first hit survives the cut"
        );
    }

    #[tokio::test]
    async fn an_unusable_return_keeps_the_merged_order() {
        let dir = TempDir::new().unwrap();
        let sources = source(
            &dir,
            vec![block_hit("a.md", 0.9, 0), block_hit("b.md", 0.5, 0)],
        );
        let stage = RerankStage::new(
            None,
            vec![plugin_vm(
                r#"
                cru.on("search:rerank", function(ctx, event)
                    return { { index = 99 }, { score = 1 } }
                end)
                "#,
            )],
        );

        let results = search(&sources, 10, &stage).await;

        let order: Vec<&str> = results.iter().map(|r| r.document_id.0.as_str()).collect();
        assert_eq!(order, vec!["a.md", "b.md"]);
    }

    #[tokio::test]
    async fn no_handler_means_no_over_fetch_and_no_change() {
        let dir = TempDir::new().unwrap();
        let sources = source(&dir, vec![block_hit("a.md", 0.9, 0)]);
        let mut stage = RerankStage::new(None, vec![plugin_vm("")]);
        stage.fanout = 5;

        let results = search(&sources, 10, &stage).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].block.as_ref().unwrap().cited.is_empty());
    }
}
