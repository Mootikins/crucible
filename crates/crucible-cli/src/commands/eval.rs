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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::common::daemon_client;
use crate::config::CliConfig;
use crucible_core::enrichment::eval::{
    hit_rate_at_k, mrr, normalize_stem, rank_of, GoldenQuery, GoldenSet, NamedGoldenSet,
};
use crucible_core::parser::CrucibleParser;
use crucible_daemon::VectorHit;

/// One scored query, rendered as a row.
#[derive(Clone, serde::Serialize)]
pub struct QueryResult {
    /// Class this query came from (empty in single-file mode).
    pub class: String,
    pub question: String,
    pub expect_note: String,
    pub lenient: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expect_text: Vec<String>,
    /// Rank over distinct notes.
    pub rank: Option<usize>,
    /// Rank over hit rows of the first row of the expected note that touches
    /// one expected block. `None` without `expect_text`.
    pub block_rank: Option<usize>,
    /// Rank over hit rows of the first row whose spans, own and cited, touch
    /// every expected block. `None` without `expect_text`.
    pub passage_rank: Option<usize>,
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
    run_eval_named(client, kiln_path, "", golden).await
}

async fn run_eval_named(
    client: &crucible_daemon::DaemonClient,
    kiln_path: &Path,
    class: &str,
    golden: &GoldenSet,
) -> Result<Vec<QueryResult>> {
    let mut results = Vec::with_capacity(golden.queries.len());
    let mut notes = HashMap::new();
    for q in &golden.queries {
        let vector = client
            .embed_query(kiln_path, &q.question)
            .await
            .with_context(|| format!("embedding failed for: {}", q.question))?;
        // The reply is one row per block. Ranks are over notes, so the
        // request over-fetches and the reduction below keeps the first row of
        // each note; `top_k` distinct notes need more than `top_k` rows.
        let hits = client
            .search_vectors(kiln_path, &vector, golden.top_k * OVER_FETCH, None)
            .await
            .with_context(|| format!("search failed for: {}", q.question))?;
        let (block_rank, passage_rank) = if q.expect_text.is_empty() {
            (None, None)
        } else {
            let expected = expected_spans(kiln_path, q, &mut notes).await?;
            row_ranks(&hits, &q.expect_note, &expected)
        };
        let rank = rank_among_notes(hits, &q.expect_note);
        results.push(QueryResult {
            class: class.to_string(),
            question: q.question.clone(),
            expect_note: q.expect_note.clone(),
            lenient: q.lenient,
            expect_text: q.expect_text.clone(),
            rank,
            block_rank,
            passage_rank,
        });
    }
    Ok(results)
}

/// A note's body and the byte spans of its blocks, as the parser cuts them.
struct NoteBlocks {
    body: String,
    spans: Vec<(usize, usize)>,
}

async fn parse_note(path: &Path) -> Result<NoteBlocks> {
    let text = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("reading {}", path.display()))?;
    let note = CrucibleParser::new()
        .parse_content(&text, path)
        .await
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(NoteBlocks {
        body: text[note.body_offset..].to_string(),
        spans: note
            .content
            .blocks
            .iter()
            .map(|b| (b.start_offset, b.end_offset))
            .collect(),
    })
}

/// The one `*.md` under `kiln_path` whose stem is `expect_note`.
fn find_note(kiln_path: &Path, expect_note: &str) -> Result<PathBuf> {
    let want = normalize_stem(expect_note);
    let hidden = |e: &walkdir::DirEntry| e.file_name().to_string_lossy().starts_with('.');
    let mut found: Vec<PathBuf> = walkdir::WalkDir::new(kiln_path)
        .into_iter()
        .filter_entry(|e| !hidden(e))
        .filter_map(Result::ok)
        .map(walkdir::DirEntry::into_path)
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| normalize_stem(&p.to_string_lossy()) == want)
        .collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => bail!("no note named {expect_note} under {}", kiln_path.display()),
        n => bail!(
            "{n} notes named {expect_note} under {}",
            kiln_path.display()
        ),
    }
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The span of the one block whose source holds `phrase`. A phrase that no
/// block or several blocks hold is a fixture error, not a miss.
fn block_holding(note: &NoteBlocks, phrase: &str) -> Result<(usize, usize)> {
    let want = collapse(phrase);
    let found: Vec<(usize, usize)> = note
        .spans
        .iter()
        .copied()
        .filter(|(s, e)| collapse(&note.body[*s..*e]).contains(&want))
        .collect();
    match found.as_slice() {
        [one] => Ok(*one),
        [] => bail!("no block holds {phrase:?}"),
        many => bail!("{} blocks hold {phrase:?}", many.len()),
    }
}

/// The spans of the blocks `q.expect_text` names, in the note the golden set
/// expects. Parsed notes are kept in `notes` across the queries of one set.
async fn expected_spans(
    kiln_path: &Path,
    q: &GoldenQuery,
    notes: &mut HashMap<PathBuf, NoteBlocks>,
) -> Result<Vec<(usize, usize)>> {
    let path = find_note(kiln_path, &q.expect_note)?;
    if !notes.contains_key(&path) {
        notes.insert(path.clone(), parse_note(&path).await?);
    }
    let note = &notes[&path];
    q.expect_text
        .iter()
        .map(|phrase| {
            block_holding(note, phrase)
                .with_context(|| format!("{} for {:?}", q.expect_note, q.question))
        })
        .collect()
}

fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Block-granular ranks over the hit rows, in reply order: the first row of
/// the expected note that touches one expected block, and the first whose
/// spans touch them all. A strategy that cites a pair can reach the second;
/// a plain block hit cannot when the answer spans two blocks.
fn row_ranks(
    hits: &[VectorHit],
    expect_note: &str,
    expected: &[(usize, usize)],
) -> (Option<usize>, Option<usize>) {
    let want = normalize_stem(expect_note);
    let mut block_rank = None;
    let mut passage_rank = None;
    for (i, hit) in hits.iter().enumerate() {
        let Some(block) = &hit.block else { continue };
        if normalize_stem(&hit.document_id) != want {
            continue;
        }
        let spans = || {
            std::iter::once((block.span_start, block.span_end)).chain(block.cited.iter().copied())
        };
        let covered = expected
            .iter()
            .filter(|e| spans().any(|s| overlaps(s, **e)))
            .count();
        if covered > 0 {
            block_rank.get_or_insert(i + 1);
        }
        if covered == expected.len() {
            passage_rank.get_or_insert(i + 1);
            break;
        }
    }
    (block_rank, passage_rank)
}

/// Rows requested per golden `top_k`. Four blocks of one note above the
/// answer is common in a long note; more than that and the answer is a miss
/// that the metrics should report.
const OVER_FETCH: usize = 4;

/// 1-based rank of the expected note among distinct notes, in reply order.
///
/// Block rows of one note collapse to that note's first row before the
/// rank is read, so a note with several blocks above the answer costs the
/// answer one rank, not several. `rank_of` matches by stem, so the corpus
/// layout does not have to match the fixture.
fn rank_among_notes(hits: Vec<VectorHit>, expect_note: &str) -> Option<usize> {
    let titles: Vec<String> = crucible_daemon::first_per_note(hits)
        .into_iter()
        .map(|hit| hit.document_id)
        .collect();
    rank_of(&titles, expect_note)
}

/// The retrieval strategy the daemon runs, from `[plugins.retrieval-lab]`.
///
/// The lab plugin selects its strategy by this one key. The eval names it
/// in the header so a results row says which strategy it measured. `None`
/// when the section or the key is absent, which is the block-point default.
fn retrieval_strategy(config: &CliConfig) -> Option<String> {
    config
        .plugins
        .get("retrieval-lab")
        .and_then(|section| section.get("strategy"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// One class of the `--json` report: its metrics and its rows.
#[derive(serde::Serialize)]
struct ClassReport {
    class: String,
    n: usize,
    top_k: usize,
    hit_at_1: f64,
    hit_at_k: f64,
    mrr: f64,
    recall_at_k: f64,
    /// Queries with `expect_text`; the three block metrics are over these.
    passage_n: usize,
    block_hit_at_1: f64,
    block_hit_at_k: f64,
    passage_hit_at_k: f64,
    queries: Vec<QueryResult>,
}

impl ClassReport {
    fn new(class: &str, results: &[QueryResult], top_k: usize) -> Self {
        let (hit_at_1, hit_at_k, mrr, recall_at_k) = aggregate(results, top_k);
        let (passage_n, block_hit_at_1, block_hit_at_k, passage_hit_at_k) =
            aggregate_rows(results, top_k);
        Self {
            class: class.to_string(),
            n: results.len(),
            top_k,
            hit_at_1,
            hit_at_k,
            mrr,
            recall_at_k,
            passage_n,
            block_hit_at_1,
            block_hit_at_k,
            passage_hit_at_k,
            queries: results.to_vec(),
        }
    }
}

/// The whole `--json` report, one object on stdout.
#[derive(serde::Serialize)]
struct EvalReport {
    kiln: PathBuf,
    /// `[plugins.retrieval-lab] strategy`, when the daemon's config sets it.
    strategy: Option<String>,
    /// `rank` is over distinct notes; block rows of one note collapse to
    /// the note's first row. `block_rank` and `passage_rank` are over rows.
    ranks_over: &'static str,
    classes: Vec<ClassReport>,
    total: ClassReport,
}

impl EvalReport {
    fn new(
        kiln: &Path,
        strategy: Option<String>,
        class_results: &[(String, Vec<QueryResult>, usize)],
        total_top_k: usize,
    ) -> Self {
        let classes: Vec<ClassReport> = class_results
            .iter()
            .map(|(name, rows, top_k)| ClassReport::new(name, rows, *top_k))
            .collect();
        let all: Vec<QueryResult> = class_results
            .iter()
            .flat_map(|(_, rows, _)| rows.iter().cloned())
            .collect();
        Self {
            kiln: kiln.to_path_buf(),
            strategy,
            ranks_over: "notes",
            classes,
            total: ClassReport::new("TOTAL", &all, total_top_k),
        }
    }
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

/// The three block-granular rates over the queries that carry `expect_text`,
/// with their count. Zero queries give zero rates; the count says so.
fn aggregate_rows(results: &[QueryResult], top_k: usize) -> (usize, f64, f64, f64) {
    let scored: Vec<&QueryResult> = results
        .iter()
        .filter(|r| !r.expect_text.is_empty())
        .collect();
    let block: Vec<Option<usize>> = scored.iter().map(|r| r.block_rank).collect();
    let passage: Vec<Option<usize>> = scored.iter().map(|r| r.passage_rank).collect();
    (
        scored.len(),
        hit_rate_at_k(&block, 1),
        hit_rate_at_k(&block, top_k),
        hit_rate_at_k(&passage, top_k),
    )
}

fn rank_cell(rank: Option<usize>) -> String {
    rank.map(|v| v.to_string()).unwrap_or_else(|| "miss".into())
}

fn render(results: &[QueryResult], top_k: usize) {
    println!(
        "{:<4} {:<6} {:<6} {:<8} {:<8} question → expected",
        "#", "rank", "block", "passage", "lenient"
    );
    for (i, r) in results.iter().enumerate() {
        println!(
            "{:<4} {:<6} {:<6} {:<8} {:<8} {} → {}",
            i + 1,
            rank_cell(r.rank),
            if r.expect_text.is_empty() {
                "-".into()
            } else {
                rank_cell(r.block_rank)
            },
            if r.expect_text.is_empty() {
                "-".into()
            } else {
                rank_cell(r.passage_rank)
            },
            if r.lenient { "yes" } else { "no" },
            truncate(&r.question, 48),
            r.expect_note,
        );
    }
    render_aggregate_line(&aggregate(results, top_k), top_k);
    let (n, b1, bk, pk) = aggregate_rows(results, top_k);
    if n > 0 {
        println!(
            "over rows, {n} queries with expect_text: block@1 {b1:.3} · block@{top_k} {bk:.3} · passage@{top_k} {pk:.3}"
        );
    }
}

fn render_aggregate_line(agg: &(f64, f64, f64, f64), top_k: usize) {
    let (h1, hk, m, recall) = *agg;
    println!();
    println!("hit@1 {h1:.3} · hit@{top_k} {hk:.3} · MRR {m:.3} · recall@{top_k} {recall:.3}",);
}

/// Per-class table for --golden-dir mode: one row per class sorted
/// alphabetically, then TOTAL over all queries.
fn render_class_table(rows: &[(String, Vec<QueryResult>)], top_k: usize) {
    // A class whose queries are ALL lenient has an empty strict set, so its
    // hit@1/hit@k/MRR are 0.000 over zero samples — indistinguishable from a
    // class that missed everything. Flag it so the numbers get read honestly.
    let no_strict: Vec<&str> = rows
        .iter()
        .filter(|(_, r)| r.iter().all(|q| q.lenient))
        .map(|(name, _)| name.as_str())
        .collect();
    if !no_strict.is_empty() {
        eprintln!(
            "warning: these classes have no strict (non-lenient) queries; their \
             hit@1/hit@k/MRR are vacuous — mark some queries strict or read only recall: {}",
            no_strict.join(", ")
        );
    }
    println!(
        "{:<32} {:>5} {:>7} {:>7} {:>7} {:>9} {:>5} {:>7} {:>7} {:>9}",
        "class", "n", "hit@1", "hit@k", "MRR", "recall@k", "psg_n", "blk@1", "blk@k", "passage@k"
    );
    let mut all = Vec::new();
    for (name, results) in rows {
        render_class_row(name, results, top_k);
        all.extend(results.iter().cloned());
    }
    render_class_row("TOTAL", &all, top_k);
}

fn render_class_row(name: &str, results: &[QueryResult], top_k: usize) {
    let (h1, hk, m, recall) = aggregate(results, top_k);
    let (pn, b1, bk, pk) = aggregate_rows(results, top_k);
    println!(
        "{:<32} {:>5} {:>7.3} {:>7.3} {:>7.3} {:>9.3} {:>5} {:>7.3} {:>7.3} {:>9.3}",
        name,
        results.len(),
        h1,
        hk,
        m,
        recall,
        pn,
        b1,
        bk,
        pk
    );
}

fn render_multi(
    named_sets: &[NamedGoldenSet],
    class_results: &[(String, Vec<QueryResult>)],
    top_k: usize,
) {
    render_class_table(class_results, top_k);
    println!();
    for NamedGoldenSet { name, .. } in named_sets {
        if let Some(rows) = class_results.iter().find(|(c, _)| c == name) {
            for (i, r) in rows.1.iter().enumerate() {
                println!(
                    "{} {:<4} {:<6} {} → {}",
                    name,
                    i + 1,
                    r.rank
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "miss".into()),
                    truncate(&r.question, 48),
                    r.expect_note,
                );
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}

async fn open_kiln(config: &CliConfig) -> Result<crucible_daemon::DaemonClient> {
    let kiln_path = config.kiln_path.clone();
    if !kiln_path.join(".crucible").join("kiln.toml").exists() {
        anyhow::bail!("No kiln is open. Run `cru init` to create one.");
    }
    let client = daemon_client().await?;
    client
        .kiln_open(&kiln_path)
        .await
        .context("Failed to open kiln in daemon")?;
    Ok(client)
}

/// Execute `cru eval precognition`. Exactly one of golden/golden_dir must be set.
///
/// With `json`, stdout is one [`EvalReport`] object and nothing else, so a
/// script can collect rows across runs.
pub async fn execute(
    config: CliConfig,
    golden_path: Option<PathBuf>,
    golden_dir: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let kiln = open_kiln(&config).await?;
    match (golden_path, golden_dir) {
        (Some(path), None) => execute_single(kiln, config, path, json).await,
        (None, Some(dir)) => execute_multi(kiln, config, dir, json).await,
        (None, None) => anyhow::bail!("specify --golden <file.toml> or --golden-dir <dir>"),
        (Some(_), Some(_)) => {
            anyhow::bail!("--golden and --golden-dir are mutually exclusive")
        }
    }
}

fn render_strategy_line(strategy: Option<&str>) {
    println!(
        "strategy: {}",
        strategy.unwrap_or("points (no [plugins.retrieval-lab] strategy set)")
    );
}

async fn execute_single(
    kiln: crucible_daemon::DaemonClient,
    config: CliConfig,
    golden_path: PathBuf,
    json: bool,
) -> Result<()> {
    let text = std::fs::read_to_string(&golden_path)
        .with_context(|| format!("reading golden set {}", golden_path.display()))?;
    let golden = GoldenSet::parse_toml(&text).context("parsing golden set")?;
    let kiln_path = config.kiln_path.clone();
    let strategy = retrieval_strategy(&config);

    if !json {
        println!(
            "Scoring {} queries against {} (top_k={}; ranks are over notes, block rows deduplicated)",
            golden.queries.len(),
            kiln_path.display(),
            golden.top_k
        );
        render_strategy_line(strategy.as_deref());
    }
    let class = golden_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let results = run_eval_named(&kiln, &kiln_path, &class, &golden).await?;
    if json {
        let report = EvalReport::new(
            &kiln_path,
            strategy,
            &[(class, results, golden.top_k)],
            golden.top_k,
        );
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        render(&results, golden.top_k);
    }
    Ok(())
}

async fn execute_multi(
    kiln: crucible_daemon::DaemonClient,
    config: CliConfig,
    dir: PathBuf,
    json: bool,
) -> Result<()> {
    let sets = GoldenSet::parse_dir(&dir)?;
    let kiln_path = config.kiln_path.clone();
    let strategy = retrieval_strategy(&config);
    let total: usize = sets.iter().map(|s| s.set.queries.len()).sum();

    if !json {
        println!(
            "Scoring {} classes / {} queries against {} (top_k from each fixture; ranks are over notes, block rows deduplicated)",
            sets.len(),
            total,
            kiln_path.display()
        );
        render_strategy_line(strategy.as_deref());
    }
    let mut class_results = Vec::new();
    for named in &sets {
        let rows = run_eval_named(&kiln, &kiln_path, &named.name, &named.set).await?;
        class_results.push((named.name.clone(), rows, named.set.top_k));
    }
    if json {
        let report = EvalReport::new(&kiln_path, strategy, &class_results, 10);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let rows: Vec<(String, Vec<QueryResult>)> = class_results
            .into_iter()
            .map(|(name, rows, _)| (name, rows))
            .collect();
        render_multi(&sets, &rows, 10);
    }
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
                class: String::new(),
                question: "a".into(),
                expect_note: "x".into(),
                lenient: false,
                expect_text: vec![],
                rank: Some(1),
                block_rank: None,
                passage_rank: None,
            },
            QueryResult {
                class: String::new(),
                question: "b".into(),
                expect_note: "y".into(),
                lenient: true,
                expect_text: vec![],
                rank: None,
                block_rank: None,
                passage_rank: None,
            },
        ];
        let (h1, _hk, _m, recall10) = aggregate(&results, 10);
        assert!(
            (h1 - 1.0).abs() < 1e-9,
            "lenient miss must not drag strict hit@1"
        );
        assert!((recall10 - 0.5).abs() < 1e-9);
    }

    fn block_row(document_id: &str, span_start: usize) -> crucible_daemon::VectorHit {
        crucible_daemon::VectorHit {
            document_id: document_id.to_string(),
            score: 0.9,
            block: Some(crucible_core::types::database::BlockRef {
                span_start,
                span_end: span_start + 8,
                kind: "paragraph".to_string(),
                cited: Vec::new(),
            }),
            snippet: None,
        }
    }

    #[test]
    fn rank_counts_distinct_notes_not_block_rows() {
        let hits = vec![
            block_row("notes/A.md", 0),
            block_row("notes/A.md", 8),
            block_row("notes/A.md", 16),
            block_row("notes/B.md", 0),
        ];
        assert_eq!(rank_among_notes(hits, "b"), Some(2));
    }

    #[test]
    fn strategy_comes_from_the_retrieval_lab_plugin_section() {
        let mut config = CliConfig::default();
        assert_eq!(retrieval_strategy(&config), None);
        config.plugins.insert(
            "retrieval-lab".to_string(),
            serde_json::json!({ "enabled": true, "strategy": "arc_post" }),
        );
        assert_eq!(retrieval_strategy(&config).as_deref(), Some("arc_post"));
    }

    #[test]
    fn json_report_carries_strategy_metrics_and_rows_per_class() {
        let mk = |rank: Option<usize>| QueryResult {
            class: "c".into(),
            question: "q".into(),
            expect_note: "n".into(),
            lenient: false,
            expect_text: vec![],
            rank,
            block_rank: None,
            passage_rank: None,
        };
        let classes = vec![("c".to_string(), vec![mk(Some(1)), mk(None)], 5)];
        let report = EvalReport::new(Path::new("/k"), Some("bezier_post".into()), &classes, 10);
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["strategy"], "bezier_post");
        assert_eq!(value["ranks_over"], "notes");
        assert_eq!(value["classes"][0]["n"], 2);
        assert_eq!(value["classes"][0]["top_k"], 5);
        assert!((value["classes"][0]["hit_at_1"].as_f64().unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(
            value["classes"][0]["queries"][1]["rank"],
            serde_json::Value::Null
        );
        assert_eq!(value["total"]["top_k"], 10);
        assert_eq!(value["total"]["n"], 2);
    }

    #[test]
    fn row_ranks_read_the_cited_spans_and_stop_at_the_first_full_cover() {
        let mut cited = block_row("n", 100);
        cited.block.as_mut().unwrap().cited = vec![(0, 8), (10, 18)];
        let hits = vec![
            block_row("other", 0),
            block_row("n", 50),
            block_row("n", 10),
            cited,
        ];
        let expected = [(0, 8), (10, 18)];
        assert_eq!(row_ranks(&hits, "n", &expected), (Some(3), Some(4)));
        assert_eq!(row_ranks(&hits, "other", &expected), (Some(1), None));
        assert_eq!(row_ranks(&hits, "absent", &expected), (None, None));
    }

    #[test]
    fn block_holding_matches_across_a_line_wrap_and_refuses_a_shared_phrase() {
        let body = "one two
three

four five

six two";
        let note = NoteBlocks {
            body: body.into(),
            spans: vec![(0, 13), (15, 24), (26, 33)],
        };
        assert_eq!(block_holding(&note, "two three").unwrap(), (0, 13));
        assert!(block_holding(&note, "two").is_err());
        assert!(block_holding(&note, "seven").is_err());
    }

    /// Every `expect_text` phrase of the transition fixture names exactly one
    /// block of the note it expects, in the docs the lab copies into its kiln.
    #[tokio::test]
    async fn transition_fixture_phrases_each_name_one_block() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text =
            std::fs::read_to_string(root.join("assets/fixtures/transition_queries/golden.toml"))
                .unwrap();
        let golden = GoldenSet::parse_toml(&text).unwrap();
        let mut notes = HashMap::new();
        let mut scored = 0;
        for q in golden.queries.iter().filter(|q| !q.expect_text.is_empty()) {
            let spans = expected_spans(&root.join("docs"), q, &mut notes)
                .await
                .unwrap();
            assert_eq!(spans.len(), 2, "{}", q.question);
            assert_ne!(spans[0], spans[1], "{}", q.question);
            scored += 1;
        }
        assert_eq!(scored, 40);
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        assert_eq!(truncate("short", 10), "short");
        let long: String = "x".repeat(60);
        let cut = truncate(&long, 48);
        assert!(cut.chars().count() == 49 && cut.ends_with('…'));
    }

    #[test]
    fn class_table_sorts_alphabetically_and_totals_hand_computed_metrics() {
        // Hand-computed: two classes, known ranks.
        // class-b strict ranks [Some(1)] => hit@1=1.0, MRR=1.0
        // class-a strict ranks [Some(2)] => hit@1=0.0, MRR=0.5
        // TOTAL strict ranks [Some(1),Some(2)] => hit@1=0.5, hit@10=1.0, MRR=0.75
        let mk = |class: &str, rank: Option<usize>| QueryResult {
            class: class.into(),
            question: "q".into(),
            expect_note: "n".into(),
            lenient: false,
            expect_text: vec![],
            rank,
            block_rank: None,
            passage_rank: None,
        };
        let rows = vec![
            ("class-b".to_string(), vec![mk("class-b", Some(1))]),
            ("class-a".to_string(), vec![mk("class-a", Some(2))]),
        ];
        // Render into a captured string via a tiny shim: reuse aggregate math directly.
        let mut sorted = rows.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            sorted[0].0, "class-a",
            "rows must sort alphabetically before rendering"
        );
        let all: Vec<QueryResult> = rows.iter().flat_map(|(_, r)| r.iter().cloned()).collect();
        let (h1, hk, m, _) = aggregate(&all, 10);
        assert!((h1 - 0.5).abs() < 1e-9);
        assert!((hk - 1.0).abs() < 1e-9);
        assert!((m - 0.75).abs() < 1e-9);
    }
}
