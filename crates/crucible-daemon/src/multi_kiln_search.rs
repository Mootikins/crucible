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
}

/// How many rows each source answers per requested row when a
/// `search:rerank` handler is registered, so a rerank has more than the
/// final cut to choose from. With no handler the search fetches `top_k`.
pub const RERANK_FANOUT: usize = 5;

impl RerankStage {
    /// A stage over `vms`.
    pub fn new(session_id: Option<String>, vms: Vec<StageVm>) -> Self {
        Self { session_id, vms }
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
    let fetch = rerank.map_or(top_k, |_| top_k.saturating_mul(RERANK_FANOUT));

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
    // Equal scores fall back to note then span, so two runs of one query
    // give one order. The map's order is per process.
    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.document_id.0.cmp(&b.document_id.0))
            .then_with(|| {
                let start = |r: &SearchResult| r.block.as_ref().map(|b| b.span_start);
                start(a).cmp(&start(b))
            })
    });

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
            let decided = !reranked.is_empty();
            let resolved = resolve_reranked(sources, reranked).await;
            if decided && resolved.is_empty() {
                tracing::warn!(
                    "search:rerank handler introduced no resolvable hit; keeping the merged order"
                );
            } else {
                merged = resolved;
            }
        }
    }
    merged.truncate(top_k);

    Ok(merged)
}

/// The source a handler names by kiln. The first of that name, as the
/// registry names are unique in a well-formed config.
fn source_named<'a>(
    sources: &'a [KilnSearchSource],
    name: &crucible_core::config::KilnName,
) -> Option<&'a KilnSearchSource> {
    sources
        .iter()
        .find(|source| source.kiln_name.as_ref() == Some(name))
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
    let mut entries = Vec::with_capacity(hits.len());
    for (position, hit) in hits.iter().enumerate() {
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
        entries.push(serde_json::Value::Object(entry));
    }
    let hits = entries;
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

/// One entry of a `search:rerank` return, after the sync checks and before
/// the block store is asked about the introduced ones.
enum Reranked {
    /// A merged hit the handler addressed by `index`, with its edits applied.
    Kept(SearchResult),
    /// A block the kilns did not return, named by kiln, note and start.
    Introduced {
        kiln: crucible_core::config::KilnName,
        path: String,
        span_start: usize,
        score: f64,
        cited: Vec<(usize, usize)>,
    },
}

/// The `(start, stop)` pairs of an entry's `cited`, when it has one.
fn cited_pairs(entry: &serde_json::Value) -> Option<Vec<(usize, usize)>> {
    let cited = entry.get("cited").and_then(lua_array)?;
    Some(
        cited
            .iter()
            .filter_map(|pair| {
                let pair = lua_array(pair)?;
                let start = pair.first()?.as_u64()? as usize;
                let stop = pair.get(1)?.as_u64()? as usize;
                (start <= stop).then_some((start, stop))
            })
            .collect(),
    )
}

/// Read a `search:rerank` Transform over the merged hits.
///
/// An entry with an `index` addresses a merged hit, so a handler can
/// reorder, rescore, widen and cite it. An entry with no `index` but with
/// `kiln`, `path`, `span_start` and `score` introduces a block the kilns did
/// not return; [`resolve_reranked`] reads it afterwards. An introduced block
/// that a merged hit already names, or that an earlier entry introduced, is
/// dropped. `None` means the value is not a list, or every entry was
/// unusable; the merged order then stands. An empty list is a decision and
/// yields no hits.
fn apply_rerank(merged: &[SearchResult], value: &serde_json::Value) -> Option<Vec<Reranked>> {
    let entries = lua_array(value)?;
    let mut seen = std::collections::HashSet::new();
    let mut introduced = std::collections::HashSet::new();
    let known: std::collections::HashSet<(Option<&crucible_core::config::KilnName>, &str, usize)> =
        merged
            .iter()
            .filter_map(|hit| {
                let block = hit.block.as_ref()?;
                Some((
                    hit.kiln.as_ref(),
                    hit.document_id.0.as_str(),
                    block.span_start,
                ))
            })
            .collect();
    let mut reranked = Vec::with_capacity(entries.len());

    for entry in &entries {
        let Some(index) = entry.get("index").and_then(|v| v.as_u64()) else {
            let kiln = entry
                .get("kiln")
                .and_then(|v| v.as_str())
                .and_then(crucible_core::config::KilnName::normalize);
            let path = entry.get("path").and_then(|v| v.as_str());
            let span_start = entry.get("span_start").and_then(|v| v.as_u64());
            let score = entry.get("score").and_then(|v| v.as_f64());
            let (Some(kiln), Some(path), Some(span_start), Some(score)) =
                (kiln, path, span_start, score)
            else {
                tracing::warn!(
                    "search:rerank entry has no `index` and no complete `kiln`, `path`, `span_start`, `score`; dropping"
                );
                continue;
            };
            let span_start = span_start as usize;
            if known.contains(&(Some(&kiln), path, span_start))
                || !introduced.insert((kiln.clone(), path.to_string(), span_start))
            {
                tracing::warn!(
                    kiln = kiln.as_str(),
                    path,
                    span_start,
                    "search:rerank introduces a block the list already has; dropping"
                );
                continue;
            }
            reranked.push(Reranked::Introduced {
                kiln,
                path: path.to_string(),
                span_start,
                score,
                cited: cited_pairs(entry).unwrap_or_default(),
            });
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
            if let Some(cited) = cited_pairs(entry) {
                block.cited = cited;
            }
        }
        reranked.push(Reranked::Kept(hit));
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

/// Turn every introduced entry into a hit by reading its block row from the
/// named kiln, in the handler's order. A kiln the search did not cover, or a
/// row the store does not have, drops the entry with a warning.
async fn resolve_reranked(
    sources: &[KilnSearchSource],
    entries: Vec<Reranked>,
) -> Vec<SearchResult> {
    let mut hits = Vec::with_capacity(entries.len());
    for entry in entries {
        let (kiln, path, span_start, score, cited) = match entry {
            Reranked::Kept(hit) => {
                hits.push(hit);
                continue;
            }
            Reranked::Introduced {
                kiln,
                path,
                span_start,
                score,
                cited,
            } => (kiln, path, span_start, score, cited),
        };
        let Some(source) = source_named(sources, &kiln) else {
            tracing::warn!(
                kiln = kiln.as_str(),
                path,
                "search:rerank introduces a block from a kiln the search did not cover; dropping"
            );
            continue;
        };
        let rows = source
            .knowledge_repo
            .blocks_for_note(&path)
            .await
            .unwrap_or_default();
        let Some(row) = rows.into_iter().find(|row| row.span_start == span_start) else {
            tracing::warn!(
                kiln = kiln.as_str(),
                path,
                span_start,
                "search:rerank introduces a block the store has no row for; dropping"
            );
            continue;
        };
        hits.push(SearchResult {
            document_id: DocumentId(path),
            score,
            highlights: None,
            snippet: Some(row.text),
            kiln: Some(kiln),
            block: Some(crucible_core::types::database::BlockRef {
                span_start: row.span_start,
                span_end: row.span_end,
                kind: row.kind,
                cited,
            }),
        });
    }
    hits
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
        let stage = RerankStage::new(None, vec![plugin_vm("")]);

        let results = search(&sources, 10, &stage).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].block.as_ref().unwrap().cited.is_empty());
    }

    /// A stored row of `path` at `span_start`, as the block store would
    /// answer it to `blocks_for_note`.
    fn stored_row(path: &str, span_start: usize) -> crucible_core::storage::BlockRecord {
        crucible_core::storage::BlockRecord {
            note_path: path.to_string(),
            span_start,
            span_end: span_start + 10,
            kind: "paragraph".to_string(),
            content_hash: crucible_core::parser::BlockHash::new([0; 32]),
            text: format!("stored passage of {path} at {span_start}"),
            embedding: Some(vec![1.0, 0.0]),
            embedding_model: Some("mock".to_string()),
            embedding_dimensions: Some(2),
        }
    }

    /// A source whose store also answers `blocks_for_note`, so a handler can
    /// introduce a row.
    fn stored_source(
        dir: &TempDir,
        hits: Vec<SearchResult>,
        rows: Vec<crucible_core::storage::BlockRecord>,
    ) -> Vec<KilnSearchSource> {
        vec![KilnSearchSource {
            kiln_path: dir.path().to_path_buf(),
            kiln_name: crucible_core::config::KilnName::normalize("lab"),
            knowledge_repo: Arc::new(
                MockKnowledgeRepository::new()
                    .with_block_results(hits)
                    .with_note_blocks(rows),
            ),
        }]
    }

    /// A handler introduces a block the search did not return; the daemon
    /// reads its row from the named kiln, and the cut counts it.
    #[tokio::test]
    async fn a_handler_introduces_a_stored_block() {
        let dir = TempDir::new().unwrap();
        let sources = stored_source(
            &dir,
            vec![block_hit("a.md", 0.9, 0), block_hit("b.md", 0.5, 0)],
            vec![stored_row("c.md", 40)],
        );
        let stage = RerankStage::new(
            None,
            vec![plugin_vm(
                r#"
                cru.on("search:rerank", function(ctx, event)
                    return {
                        { kiln = "lab", path = "c.md", span_start = 40, score = 0.95, cited = { { 0, 5 } } },
                        { index = 1 },
                        { index = 2 },
                    }
                end)
                "#,
            )],
        );

        let results = search(&sources, 2, &stage).await;

        assert_eq!(results.len(), 2, "the cut counts the introduced hit");
        let first = &results[0];
        assert_eq!(first.document_id.0, "c.md");
        assert_eq!(first.score, 0.95);
        assert_eq!(
            first.kiln,
            crucible_core::config::KilnName::normalize("lab")
        );
        assert_eq!(
            first.snippet.as_deref(),
            Some("stored passage of c.md at 40")
        );
        let block = first.block.as_ref().unwrap();
        assert_eq!((block.span_start, block.span_end), (40, 50));
        assert_eq!(block.cited, vec![(0, 5)]);
        assert_eq!(results[1].document_id.0, "a.md");
    }

    /// An introduced entry that names another kiln, a row the store lacks,
    /// a block a merged hit already names, or a block already introduced is
    /// dropped; the entries around it stand.
    #[tokio::test]
    async fn an_introduced_block_the_search_cannot_place_is_dropped() {
        let dir = TempDir::new().unwrap();
        let sources = stored_source(
            &dir,
            vec![block_hit("a.md", 0.9, 0)],
            vec![stored_row("c.md", 40)],
        );
        let stage = RerankStage::new(
            None,
            vec![plugin_vm(
                r#"
                cru.on("search:rerank", function(ctx, event)
                    return {
                        { kiln = "other", path = "c.md", span_start = 40, score = 1 },
                        { kiln = "lab", path = "c.md", span_start = 7, score = 1 },
                        { kiln = "lab", path = "a.md", span_start = 0, score = 1 },
                        { kiln = "lab", path = "c.md", span_start = 40, score = 0.8 },
                        { kiln = "lab", path = "c.md", span_start = 40, score = 0.7 },
                        { index = 1 },
                    }
                end)
                "#,
            )],
        );

        let results = search(&sources, 10, &stage).await;

        let order: Vec<(&str, f64)> = results
            .iter()
            .map(|r| (r.document_id.0.as_str(), r.score))
            .collect();
        assert_eq!(order, vec![("c.md", 0.8), ("a.md", 0.9)]);
    }

    /// With a handler registered each source answers `top_k * RERANK_FANOUT`
    /// rows, and the reply is still cut to `top_k`. Without one the source
    /// answers `top_k`.
    #[tokio::test]
    async fn a_registered_handler_over_fetches_and_the_cut_still_holds() {
        let dir = TempDir::new().unwrap();
        let hits: Vec<SearchResult> = (0..12)
            .map(|i| block_hit("a.md", 1.0 - i as f64 / 100.0, i * 10))
            .collect();
        let repo = Arc::new(MockKnowledgeRepository::new().with_block_results(hits));
        let sources = vec![KilnSearchSource {
            kiln_path: dir.path().to_path_buf(),
            kiln_name: crucible_core::config::KilnName::normalize("lab"),
            knowledge_repo: repo.clone(),
        }];
        let stage = RerankStage::new(
            None,
            vec![plugin_vm(
                r#"cru.on("search:rerank", function(ctx, event) return nil end)"#,
            )],
        );

        let results = search(&sources, 2, &stage).await;
        assert_eq!(repo.block_limits(), vec![2 * RERANK_FANOUT]);
        assert_eq!(results.len(), 2);

        let silent = RerankStage::new(None, vec![plugin_vm("")]);
        search(&sources, 2, &silent).await;
        assert_eq!(repo.block_limits(), vec![2 * RERANK_FANOUT, 2]);
    }
}
