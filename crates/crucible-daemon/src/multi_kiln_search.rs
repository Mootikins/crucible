use crate::trust_resolution::resolve_session_classification;
use anyhow::Result;
use crucible_core::config::{DataClassification, TrustLevel};
use crucible_core::traits::KnowledgeRepository;
use crucible_core::{DocumentId, SearchResult};
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

pub async fn search_across_kilns(
    sources: &[KilnSearchSource],
    query_embedding: Vec<f32>,
    top_k: usize,
    provider_trust: Option<TrustLevel>,
    workspace: Option<&Path>,
) -> Result<Vec<SearchResult>> {
    let mut best: HashMap<(PathBuf, String), SearchResult> = HashMap::new();

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
        let results = match source
            .knowledge_repo
            .search_vectors(query_embedding.clone(), top_k)
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
            // The name, not the path. The dedup key below still uses the path —
            // it is the identity of the corpus, and two kilns may share a name
            // in a malformed config — but nothing that leaves this function
            // carries one.
            result.kiln = source.kiln_name.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::parser::ParsedNote;
    use crucible_core::traits::knowledge::NoteInfo;
    use std::fs;
    use tempfile::TempDir;

    struct MockKnowledgeRepository {
        results: Vec<SearchResult>,
        should_fail: bool,
    }

    #[async_trait]
    impl KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(&self, _name: &str) -> crucible_core::Result<Option<ParsedNote>> {
            Ok(None)
        }

        async fn list_notes(&self, _path: Option<&str>) -> crucible_core::Result<Vec<NoteInfo>> {
            Ok(vec![])
        }

        async fn search_vectors(
            &self,
            _vector: Vec<f32>,
            _limit: usize,
        ) -> crucible_core::Result<Vec<SearchResult>> {
            if self.should_fail {
                Err(crucible_core::CrucibleError::DatabaseError(
                    "mock failure".into(),
                ))
            } else {
                Ok(self.results.clone())
            }
        }
    }

    fn mock_result(document_id: &str, score: f64) -> SearchResult {
        SearchResult {
            document_id: DocumentId(document_id.to_string()),
            score,
            highlights: None,
            snippet: None,
            kiln: None,
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
            knowledge_repo: Arc::new(MockKnowledgeRepository {
                results,
                should_fail,
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
            knowledge_repo: Arc::new(MockKnowledgeRepository {
                results,
                should_fail,
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
}
