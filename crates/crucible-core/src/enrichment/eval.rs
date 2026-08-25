//! Golden-set types and retrieval metrics for the precognition eval.
//!
//! A golden set is a list of (query, expected note title) pairs over a known
//! corpus. The metrics are pure functions of a ranking, so they live here —
//! canonical location, no daemon dependency — and both the CLI command and any
//! future harness re-export them.

use serde::{Deserialize, Serialize};

/// One eval question: what was asked and which note should surface for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenQuery {
    /// The natural-language question as a user would type it.
    pub question: String,
    /// Filename stem of the note that should rank for this question
    /// (e.g. `work-from-home-tips` for `Work From Home Tips.md`).
    pub expect_note: String,
    /// False when the match is defensible but not unambiguous: counts toward
    /// recall@k, excluded from strict hit rate. Defaults to false.
    #[serde(default)]
    pub lenient: bool,
}

/// The whole fixture file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenSet {
    #[serde(default = "GoldenSet::default_top_k")]
    pub top_k: usize,
    pub queries: Vec<GoldenQuery>,
}

impl GoldenSet {
    fn default_top_k() -> usize {
        10
    }

    /// Parse a golden set from TOML text.
    pub fn parse_toml(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }
}

// ─── Metrics ────────────────────────────────────────────────────────────────

/// 1-based rank at which the expected note appeared; `None` on a miss.
///
/// Matching is by filename stem: the corpus may store the note as
/// `subdir/Work From Home Tips.md`, the golden set names `work-from-home-tips`,
/// and both normalize to the same comparison key.
pub fn rank_of(results: &[String], expect_note: &str) -> Option<usize> {
    let want = normalize_stem(expect_note);
    results
        .iter()
        .position(|r| normalize_stem(r) == want)
        .map(|i| i + 1)
}

/// Filename stem, lowercased, separators folded to hyphens, extension stripped.
fn normalize_stem(name: &str) -> String {
    let no_ext = name.strip_suffix(".md").unwrap_or(name);
    let stem = no_ext.rsplit(['/', '\\']).next().unwrap_or(no_ext);
    stem.to_lowercase()
        .replace([' ', '_'], "-")
}

/// Did the expected note appear in the top `k`?
pub fn hit_at_k(rank: Option<usize>, k: usize) -> bool {
    matches!(rank, Some(r) if r <= k)
}

/// Mean reciprocal rank: average of 1/rank over queries that hit, misses
/// contributing zero. Empty input returns 0.0 rather than NaN.
pub fn mrr(ranks: &[Option<usize>]) -> f64 {
    if ranks.is_empty() {
        return 0.0;
    }
    let sum: f64 = ranks
        .iter()
        .map(|r| r.map(|v| 1.0 / v as f64).unwrap_or(0.0))
        .sum();
    sum / ranks.len() as f64
}

/// Fraction of queries whose expected note landed in the top `k`.
/// Empty input returns 0.0 rather than NaN.
pub fn hit_rate_at_k(ranks: &[Option<usize>], k: usize) -> f64 {
    if ranks.is_empty() {
        return 0.0;
    }
    let hits = ranks.iter().filter(|r| hit_at_k(**r, k)).count();
    hits as f64 / ranks.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_of_finds_exact_position() {
        let results = vec![
            "Tags.md".to_string(),
            "Work From Home Tips.md".to_string(),
            "Kilns.md".to_string(),
        ];
        assert_eq!(rank_of(&results, "work-from-home-tips"), Some(2));
    }

    #[test]
    fn rank_of_normalizes_separators_and_case() {
        let results = vec!["notes/remote-work_tips.md".to_string()];
        assert_eq!(rank_of(&results, "Remote Work Tips"), Some(1));
    }

    #[test]
    fn rank_of_miss_returns_none() {
        assert_eq!(rank_of(&["Tags.md".to_string()], "kilns"), None);
    }

    #[test]
    fn hit_at_k_bounds_are_inclusive_and_exclusive() {
        assert!(hit_at_k(Some(5), 5));
        assert!(!hit_at_k(Some(6), 5));
        assert!(!hit_at_k(None, 5));
    }

    #[test]
    fn mrr_averages_reciprocal_ranks_with_misses_as_zero() {
        let ranks = vec![Some(1), Some(3), None];
        assert!((mrr(&ranks) - (1.0 + 1.0 / 3.0) / 3.0).abs() < 1e-9);
    }

    #[test]
    fn mrr_of_nothing_is_zero_not_nan() {
        assert_eq!(mrr(&[]), 0.0);
        assert_eq!(hit_rate_at_k(&[], 10), 0.0);
    }

    #[test]
    fn hit_rate_counts_fraction_within_k() {
        let ranks = vec![Some(1), Some(7), None, Some(2)];
        assert!((hit_rate_at_k(&ranks, 5) - 0.5).abs() < 1e-9);
    }

    // ---- Red-proofed gate: golden set parsing must reject garbage ----

    #[test]
    fn golden_set_parses_toml_fixture_shape() {
        let text = r#"
top_k = 5

[[queries]]
question = "how do I focus remote?"
expect_note = "work-from-home-tips"

[[queries]]
question = "lenient one"
expect_note = "maybe-this"
lenient = true
"#;
        let gs = GoldenSet::parse_toml(text).expect("parses");
        assert_eq!(gs.top_k, 5);
        assert_eq!(gs.queries.len(), 2);
        assert!(!gs.queries[0].lenient);
        assert!(gs.queries[1].lenient);
    }

    #[test]
    fn golden_set_rejects_query_missing_expect_note() {
        let text = r#"
[[queries]]
question = "no answer named"
"#;
        assert!(GoldenSet::parse_toml(text).is_err());
    }
}
