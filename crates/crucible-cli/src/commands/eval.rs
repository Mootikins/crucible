//! `cru eval precognition` — score the retrieval path against a golden set.
//!
//! This is the measurement half of the precognition thesis. It runs each
//! golden query through the same two RPCs the live injection path uses
//! (`embed.query` then `search_vectors`), ranks the expected note, and reports
//! hit@1/hit@k, MRR and per-query results.
//!
//! Domain logic stays out of here by design: parsing and metrics live in
//! `crucible_core::enrichment::eval`, and retrieval is the daemon's existing
//! search surface. This file only orchestrates and renders.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::common::daemon_client;
use crate::config::CliConfig;
use crucible_core::enrichment::eval::{hit_rate_at_k, mrr, rank_of, GoldenSet};

/// One scored query, rendered as a row.
pub struct QueryResult {
    question: String,
    expect_note: String,
    lenient: bool,
    rank: Option<usize>,
}

/// Run every golden query against the kiln's semantic search.
///
/// Public for the e2e test; takes the pieces it needs rather than `CliConfig`
/// so the test can point it at a hermetic daemon.
pub async fn run_eval(
    client: &crucible_daemon::DaemonClient,
    kiln_path: &Path,
    golden: &GoldenSet,
) -> Result<Vec<QueryResult>> {
    let mut results = Vec::with_capacity(golden.queries.len());
    for q in &golden.queries {
        let vector = client
            .embed_query(kiln_path, &q.question)
            .await
            .with_context(|| format!("embedding failed for: {}", q.question))?;
        let hits = client
            .search_vectors(kiln_path, &vector, golden.top_k, None)
            .await
            .with_context(|| format!("search failed for: {}", q.question))?;
        // The daemon returns (document_id, score); matching is by stem so
        // directory layout in the corpus does not have to match the fixture.
        let titles: Vec<String> = hits.iter().map(|(doc_id, _)| doc_id.clone()).collect();
        let rank = rank_of(&titles, &q.expect_note);
        results.push(QueryResult {
            question: q.question.clone(),
            expect_note: q.expect_note.clone(),
            lenient: q.lenient,
            rank,
        });
    }
    Ok(results)
}

fn aggregate(results: &[QueryResult], top_k: usize) -> (f64, f64, f64, f64) {
    // Strict excludes lenient queries; recall counts everything.
    let strict_ranks: Vec<Option<usize>> = results
        .iter()
        .filter(|r| !r.lenient)
        .map(|r| r.rank)
        .collect();
    let all_ranks: Vec<Option<usize>> = results.iter().map(|r| r.rank).collect();
    (
        hit_rate_at_k(&strict_ranks, 1),
        hit_rate_at_k(&strict_ranks, top_k),
        mrr(&strict_ranks),
        hit_rate_at_k(&all_ranks, top_k),
    )
}

fn render(results: &[QueryResult], top_k: usize) {
    println!(
        "{:<4} {:<6} {:<8} question → expected",
        "#", "rank", "lenient"
    );
    for (i, r) in results.iter().enumerate() {
        println!(
            "{:<4} {:<6} {:<8} {} → {}",
            i + 1,
            r.rank
                .map(|v| v.to_string())
                .unwrap_or_else(|| "miss".into()),
            if r.lenient { "yes" } else { "no" },
            truncate(&r.question, 48),
            r.expect_note,
        );
    }
    let (h1, hk, m, recall) = aggregate(results, top_k);
    println!();
    println!("hit@1 {h1:.3} · hit@{top_k} {hk:.3} · MRR {m:.3} · recall@{top_k} {recall:.3}",);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}

/// Execute `cru eval precognition`.
pub async fn execute(config: CliConfig, golden_path: PathBuf) -> Result<()> {
    let text = std::fs::read_to_string(&golden_path)
        .with_context(|| format!("reading golden set {}", golden_path.display()))?;
    let golden = GoldenSet::parse_toml(&text).context("parsing golden set")?;

    let kiln_path = config.kiln_path.clone();
    if !kiln_path.join(".crucible").join("kiln.toml").exists() {
        anyhow::bail!("No kiln is open. Run `cru init` to create one.");
    }

    let client = daemon_client().await?;
    client
        .kiln_open(&kiln_path)
        .await
        .context("Failed to open kiln in daemon")?;

    println!(
        "Scoring {} queries against {} (top_k={})",
        golden.queries.len(),
        kiln_path.display(),
        golden.top_k
    );
    let results = run_eval(&client, &kiln_path, &golden).await?;
    render(&results, golden.top_k);
    Ok(())
}

// =============================================================================
// Tests — CLI parse surface + pure rendering helpers
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_strict_excludes_lenient_queries() {
        let results = vec![
            QueryResult {
                question: "a".into(),
                expect_note: "x".into(),
                lenient: false,
                rank: Some(1),
            },
            QueryResult {
                question: "b".into(),
                expect_note: "y".into(),
                lenient: true,
                rank: None,
            },
        ];
        let (h1, _hk, _m, recall10) = aggregate(&results, 10);
        assert!(
            (h1 - 1.0).abs() < 1e-9,
            "lenient miss must not drag strict hit@1"
        );
        assert!((recall10 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        assert_eq!(truncate("short", 10), "short");
        let long: String = "x".repeat(60);
        let cut = truncate(&long, 48);
        assert!(cut.chars().count() == 49 && cut.ends_with('…'));
    }
}
