//! Golden-set types and retrieval metrics for the precognition eval.
//!
//! A golden set is a list of (query, expected note title) pairs over a known
//! corpus. The metrics are pure functions of a ranking, so they live here —
//! canonical location, no daemon dependency — and both the CLI command and any
//! future harness re-export them.

use anyhow::Context as _;
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

    /// Parse every `*.toml` in `dir` as a golden set, sorted by filename.
    ///
    /// The class name is the file stem. An empty (or toml-less) directory is
    /// an error naming the directory; a malformed file is an error naming that
    /// file — one bad fixture should not silently shrink the eval.
    pub fn parse_dir(dir: &std::path::Path) -> anyhow::Result<Vec<NamedGoldenSet>> {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .with_context(|| format!("reading golden dir {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "toml"))
            .collect();
        entries.sort();
        if entries.is_empty() {
            anyhow::bail!("no *.toml golden-set files found in {}", dir.display());
        }
        entries
            .into_iter()
            .map(|path| {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .context("golden filename must be valid UTF-8")?
                    .to_string();
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading golden set {}", path.display()))?;
                let set = GoldenSet::parse_toml(&text)
                    .with_context(|| format!("parsing {}", path.display()))?;
                Ok(NamedGoldenSet { name, set })
            })
            .collect()
    }
}

/// A golden set plus the class name it was filed under (its file stem).
#[derive(Debug, Clone)]
pub struct NamedGoldenSet {
    pub name: String,
    pub set: GoldenSet,
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
    stem.to_lowercase().replace([' ', '_'], "-")
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

    // ---- parse_dir: multi-file golden directories ----

    use std::fs;
    use tempfile::TempDir;

    fn write_toml(dir: &std::path::Path, name: &str, text: &str) {
        fs::write(dir.join(name), text).expect("write golden toml");
    }

    #[test]
    fn parse_dir_loads_every_toml_sorted_by_filename_with_stem_names() {
        let dir = TempDir::new().expect("tmpdir");
        write_toml(
            dir.path(),
            "b-second.toml",
            "[[queries]]\nquestion = \"q2\"\nexpect_note = \"n2\"\n",
        );
        write_toml(
            dir.path(),
            "a-first.toml",
            "top_k = 3\n[[queries]]\nquestion = \"q1\"\nexpect_note = \"n1\"\n",
        );
        // Non-toml files are ignored.
        write_toml(dir.path(), "notes.txt", "not a golden set");

        let sets = GoldenSet::parse_dir(dir.path()).expect("parses");
        let names: Vec<&str> = sets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a-first", "b-second"]);
        assert_eq!(sets[0].set.top_k, 3);
        assert_eq!(sets[0].set.queries.len(), 1);
        assert_eq!(sets[1].set.queries[0].question, "q2");
    }

    #[test]
    fn parse_dir_empty_directory_error_names_the_path() {
        let dir = TempDir::new().expect("tmpdir");
        let err = GoldenSet::parse_dir(dir.path()).expect_err("empty dir must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains(dir.path().to_str().unwrap()),
            "error must name the directory: {msg}"
        );
    }

    #[test]
    fn parse_dir_malformed_file_error_names_that_file() {
        let dir = TempDir::new().expect("tmpdir");
        write_toml(
            dir.path(),
            "good.toml",
            "[[queries]]\nquestion = \"q\"\nexpect_note = \"n\"\n",
        );
        write_toml(dir.path(), "broken.toml", "this is not = valid toml [[");
        let err = GoldenSet::parse_dir(dir.path()).expect_err("malformed file must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("broken.toml"),
            "error must name the offending file: {msg}"
        );
    }
}
