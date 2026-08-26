//! `cru eval precognition` end-to-end against a hermetic daemon.
//!
//! The eval command is the measurement half of the precognition thesis: it
//! must run the same retrieval path live injection uses (`embed.query` →
//! `search_vectors`) and score a golden set. This test crosses the process
//! boundary with the `mock` embedding backend — deterministic hash vectors, no
//! network — so what it proves is that the harness works end-to-end and its
//! metrics are computed from real search results. It deliberately does NOT
//! assert semantic quality: hash embeddings carry no meaning, so any ranking
//! they produce is arbitrary. Quality numbers come from real-provider runs.

#[allow(dead_code)]
mod cli_e2e_helpers;

use cli_e2e_helpers::TestDaemon;
use std::path::PathBuf;

const MOCK_EMBEDDINGS: &str = concat!(
    "\n[enrichment.provider]\n",
    "type = \"mock\"\n",
    "dimensions = 384\n",
);

/// Seed the daemon's kiln with three distinguishable notes.
fn seed_kiln(daemon: &TestDaemon) -> PathBuf {
    let kiln = daemon
        .config_path
        .parent()
        .expect("config lives in the daemon's temp home")
        .join("kiln");

    // The CLI refuses a command without a kiln marker, same as `cru search`.
    std::fs::create_dir_all(kiln.join(".crucible")).expect("create .crucible");
    std::fs::write(kiln.join(".crucible").join("kiln.toml"), "").expect("write kiln.toml");

    std::fs::write(
        kiln.join("Kilns.md"),
        "---\ntags: [concept]\n---\n# Kilns\n\nA kiln is where accrued knowledge goes.\n",
    )
    .expect("write Kilns.md");
    std::fs::write(
        kiln.join("Wikilinks.md"),
        "---\ntags: [howto]\n---\n# Wikilinks\n\nDouble brackets connect two notes.\n",
    )
    .expect("write Wikilinks.md");
    std::fs::write(
        kiln.join("Sourdough.md"),
        "# Sourdough\n\nFeed the starter daily before baking bread.\n",
    )
    .expect("write Sourdough.md");

    kiln
}

/// A tiny golden set written into the test's own space.
fn write_golden(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("golden.toml");
    std::fs::write(
        &path,
        "top_k = 3\n\
         \n\
         [[queries]]\n\
         question = \"what is a kiln?\"\n\
         expect_note = \"Kilns\"\n\
         \n\
         [[queries]]\n\
         question = \"brackets between notes\"\n\
         expect_note = \"Wikilinks\"\n",
    )
    .expect("write golden.toml");
    path
}

/// The command must complete against a live daemon and report one row per
/// query plus the aggregate line. With mock embeddings we can only pin shape:
/// both queries are listed, each either ranked or a miss, and the aggregate
/// line renders.
#[tokio::test]
async fn eval_precognition_runs_end_to_end_against_hermetic_daemon() {
    let daemon = TestDaemon::start_with_extra_config(MOCK_EMBEDDINGS);
    let kiln = seed_kiln(&daemon);

    // Index the notes so vectors exist to search.
    let process = daemon
        .command()
        .arg("process")
        .arg(&kiln)
        .output()
        .expect("run cru process");
    assert!(
        process.status.success(),
        "cru process failed: {}",
        String::from_utf8_lossy(&process.stderr)
    );

    let golden = write_golden(kiln.parent().unwrap());

    let output = daemon
        .command()
        .current_dir(&kiln)
        .args(["eval", "precognition"])
        .arg("--golden")
        .arg(&golden)
        .output()
        .expect("run cru eval precognition");

    assert!(
        output.status.success(),
        "eval failed: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Scoring 2 queries"),
        "header missing:\n{stdout}"
    );
    assert_eq!(
        stdout.matches("what is a kiln?").count(),
        1,
        "each query appears exactly once"
    );
    assert!(
        stdout.contains("hit@1"),
        "aggregate line missing:\n{stdout}"
    );
    assert!(stdout.contains("recall@"), "recall line missing:\n{stdout}");
}

/// A golden query naming a note that is not in the corpus must be reported as
/// a miss, not an error — misses are data in an eval.
#[tokio::test]
async fn eval_reports_miss_for_note_absent_from_corpus() {
    let daemon = TestDaemon::start_with_extra_config(MOCK_EMBEDDINGS);
    let kiln = seed_kiln(&daemon);

    let process = daemon
        .command()
        .arg("process")
        .arg(&kiln)
        .output()
        .expect("run cru process");
    assert!(process.status.success());

    let golden = kiln.parent().unwrap().join("golden-miss.toml");
    std::fs::write(
        &golden,
        "top_k = 3\n\
         \n\
         [[queries]]\n\
         question = \"anything at all\"\n\
         expect_note = \"no-such-note\"\n",
    )
    .expect("write golden");
    let _ = kiln.clone();

    let output = daemon
        .command()
        .current_dir(&kiln)
        .args(["eval", "precognition", "--golden"])
        .arg(&golden)
        .output()
        .expect("run eval");

    assert!(output.status.success(), "a miss must not be an error");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("miss"), "expected miss row:\n{stdout}");
}
