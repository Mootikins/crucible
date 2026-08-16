//! `cru chat -q` must hand the daemon the user's question, unaltered.
//!
//! The CLI used to prepend its own knowledge-base block to the prompt while the
//! daemon's Precognition ran as well — on the already-prepended text. Two
//! things went wrong at once: the message persisted as "what the user said" was
//! the CLI's block, and the daemon's semantic search was issued against that
//! block instead of the question.
//!
//! The persisted session directory is the seam. `session.jsonl` carries both
//! `user_message` and `precognition_complete` and is what `cru session show`
//! replays; `meta.json` carries the agent config, which is where the two
//! one-shot context flags have to land now that the daemon owns grounding.

// Shared fixture module: this test binary needs only `TestDaemon`, so the rest
// of the helpers are dead here and live in the files that do use them.
#[allow(dead_code)]
mod cli_e2e_helpers;

use cli_e2e_helpers::TestDaemon;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Extra config for the daemon under test. `write_config` appends this
/// immediately after `default_model`, i.e. still inside
/// `[llm.providers.ollama]`, so the first two lines configure that provider.
///
/// - The closed endpoint points chat at a port nothing listens on, so the model
///   call fails immediately instead of reaching an Ollama the developer happens
///   to be running. Precognition runs *before* the model call, so the session
///   directory is complete either way — these tests never assert on success.
/// - The `mock` embedding provider is a real, non-`cfg(test)` backend
///   (`llm/embeddings/mock.rs`), which is what lets Precognition run at all
///   without a network embedding service.
const CLOSED_LLM_AND_MOCK_EMBEDDINGS: &str = concat!(
    "endpoint = \"http://127.0.0.1:9\"\n",
    "timeout_secs = 2\n",
    "\n[enrichment.provider]\n",
    "type = \"mock\"\n",
    "dimensions = 384\n",
);

const QUESTION: &str = "what is a kiln?";

/// One `cru chat <flags> <question>` run against its own daemon.
///
/// The daemon and the workspace TempDir are returned so the caller keeps them
/// alive; dropping the daemon kills it and takes the kiln with it.
struct OneShotRun {
    /// The daemon's flat sessions root — sessions no longer live in the kiln.
    sessions: PathBuf,
    output: Output,
    _daemon: TestDaemon,
    _workspace: tempfile::TempDir,
}

fn run_one_shot(extra_args: &[&str]) -> OneShotRun {
    let daemon = TestDaemon::start_with_extra_config(CLOSED_LLM_AND_MOCK_EMBEDDINGS);
    let kiln = daemon
        .config_path
        .parent()
        .expect("config lives in the daemon's temp home")
        .join("kiln");

    // One note, so the search has something to return and the assertions are
    // about a Precognition pass that actually did work.
    std::fs::write(
        kiln.join("Kilns.md"),
        "---\ntags: [concept]\n---\n# Kilns\n\nA kiln is where accrued knowledge goes.\n",
    )
    .expect("write note");

    // The workspace must be outside the hermetic HOME: the daemon refuses a
    // session workspace that is HOME or an ancestor of it.
    let workspace = tempfile::tempdir().expect("workspace temp dir");

    let output = daemon
        .command()
        .current_dir(workspace.path())
        .arg("chat")
        .args(extra_args)
        .arg(QUESTION)
        .output()
        .expect("run cru chat");

    OneShotRun {
        sessions: daemon.sessions_root(),
        output,
        _daemon: daemon,
        _workspace: workspace,
    }
}

/// The single session directory the run created. Each run gets a fresh daemon
/// and kiln, so "the only one" is well defined — no timestamp guessing.
///
/// Polled rather than read once: `cru chat -q` exiting does not mean the daemon
/// has finished writing. The session log is written by the daemon's own
/// `SessionWriter` task, so the directory can appear milliseconds after the
/// client process is already gone. Read-once passed on a quiet box and failed
/// intermittently under load with `got []` — which is how this landed as a flake
/// in the new `just test gated` tier rather than being caught when it was
/// written, since nothing ran it.
fn sole_session_dir(run: &OneShotRun) -> PathBuf {
    let sessions = &run.sessions;
    // 60s is a deadlock detector, not synchronisation: the loop leaves as soon as
    // the directory appears, polling every 25ms, so a generous ceiling costs a
    // fast box nothing. 10s was not generous enough — it failed at 10.86s in the
    // first full `just ci` run that included the gated tier, where 72 tests
    // contend and the daemon binary is cold, while passing in 0.56s alone.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut dirs: Vec<PathBuf> = Vec::new();
    loop {
        if let Ok(entries) = std::fs::read_dir(sessions) {
            dirs = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("session.jsonl").is_file())
                .collect();
        }
        if dirs.len() == 1 || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    dirs.sort();
    assert_eq!(
        dirs.len(),
        1,
        "expected exactly one session under {}, got {dirs:?}. cru exited {:?}, \
         stderr:\n{}",
        sessions.display(),
        run.output.status.code(),
        stderr(run)
    );
    dirs.pop().unwrap()
}

fn read_json(path: &Path) -> serde_json::Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// `session.jsonl` events, in write order.
fn transcript(session_dir: &Path) -> Vec<serde_json::Value> {
    let jsonl = session_dir.join("session.jsonl");
    std::fs::read_to_string(&jsonl)
        .unwrap_or_else(|e| panic!("read {}: {e}", jsonl.display()))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("session.jsonl line is JSON"))
        .collect()
}

fn find_event<'a>(events: &'a [serde_json::Value], name: &str) -> Option<&'a serde_json::Value> {
    events.iter().find(|e| e["event"] == name)
}

fn expect_event<'a>(events: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    find_event(events, name).unwrap_or_else(|| {
        let seen: Vec<&str> = events.iter().filter_map(|e| e["event"].as_str()).collect();
        // The whole transcript, not just the names. A `["user_message",
        // "ended"]` failure means the turn stopped early, and the reason is in
        // `ended`'s payload — printing names alone hid that through several
        // rounds of diagnosis.
        let full = serde_json::to_string_pretty(&events).unwrap_or_default();
        panic!("no `{name}` event in transcript; saw {seen:?}\nfull transcript:\n{full}")
    })
}

fn stderr(run: &OneShotRun) -> String {
    String::from_utf8_lossy(&run.output.stderr).into_owned()
}

#[test]
#[ignore = "requires: cru binary"]
fn one_shot_chat_sends_the_user_question_as_the_precognition_query() {
    let run = run_one_shot(&[]);
    let events = transcript(&sole_session_dir(&run));

    let precognition = expect_event(&events, "precognition_complete");
    assert_eq!(
        precognition["data"]["query_summary"].as_str(),
        Some(QUESTION),
        "the daemon's Precognition search must be issued against the user's \
         question. A `# Context from Knowledge Base` / `# User Query` prefix here \
         means the CLI enriched the prompt first and the daemon searched using \
         the CLI's own context block as its query text. cru stderr:\n{}",
        stderr(&run)
    );

    let user_message = expect_event(&events, "user_message");
    assert_eq!(
        user_message["data"]["content"].as_str(),
        Some(QUESTION),
        "the persisted user message must be verbatim what the user typed, not a \
         client-side context block. cru stderr:\n{}",
        stderr(&run)
    );
}

#[test]
#[ignore = "requires: cru binary"]
fn one_shot_context_flags_become_daemon_session_state() {
    // `--no-context` disables Precognition for the session rather than skipping
    // a local transform, so the proof is twofold: the stored agent config, and
    // no `precognition_complete` in the transcript.
    let off = run_one_shot(&["--no-context"]);
    let off_session = sole_session_dir(&off);
    let off_agent = read_json(&off_session.join("meta.json"))["agent"].clone();
    assert_eq!(
        off_agent["precognition_enabled"].as_bool(),
        Some(false),
        "--no-context must reach the daemon as session.set_precognition(false). \
         cru stderr:\n{}",
        stderr(&off)
    );
    let off_events = transcript(&off_session);
    assert!(
        find_event(&off_events, "precognition_complete").is_none(),
        "--no-context must stop the daemon grounding the turn at all, but a \
         precognition_complete event was emitted"
    );

    // `--context-size N` is the result count Precognition retrieves. Unset, the
    // daemon's own default (5) must stand — the flag used to carry clap's
    // `default_value = "5"`, which would have overwritten a resumed session's
    // stored value with 5 on every one-shot turn.
    let sized = run_one_shot(&["--context-size", "3"]);
    let sized_agent = read_json(&sole_session_dir(&sized).join("meta.json"))["agent"].clone();
    assert_eq!(
        sized_agent["precognition_results"].as_u64(),
        Some(3),
        "--context-size must reach the daemon as \
         session.set_precognition_results. cru stderr:\n{}",
        stderr(&sized)
    );
    assert_eq!(
        sized_agent["precognition_enabled"].as_bool(),
        Some(true),
        "--context-size must not disable grounding"
    );
}
