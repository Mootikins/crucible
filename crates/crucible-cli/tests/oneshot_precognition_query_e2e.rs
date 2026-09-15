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
//!
//! The daemon writes both files after the client is gone. To read them, a test
//! first stops the daemon and waits for its process to exit; see
//! [`stop_daemon_and_wait`].

// Shared fixture module: this test binary needs only `TestDaemon`, so the rest
// of the helpers are dead here and live in the files that do use them.
#[allow(dead_code)]
mod cli_e2e_helpers;

use cli_e2e_helpers::TestDaemon;
use std::io::Read;
use std::os::unix::net::UnixStream;
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
    "cru.config.set({\n",
    "  llm = { providers = { ollama = { endpoint = \"http://127.0.0.1:9\", timeout_secs = 2 } } },\n",
    "  enrichment = { provider = { type = \"mock\", dimensions = 384 } },\n",
    "})\n",
);

const QUESTION: &str = "what is a kiln?";

/// One `cru chat <flags> <question>` run against its own daemon.
///
/// The daemon process is already gone when this is returned. The `TestDaemon`
/// is kept for its temp dir, which holds the session files the assertions
/// read; dropping it deletes them.
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
    let cards = workspace.path().join(".crucible/agents");
    std::fs::create_dir_all(&cards).unwrap();
    std::fs::write(cards.join("researcher.md"), "---\nname: researcher\ndescription: Grounded research\nmodel: llama3.2\n---\n\nKeep the kiln authoritative.\n").unwrap();

    let output = daemon
        .command()
        .current_dir(workspace.path())
        .arg("chat")
        .args(extra_args)
        .arg(QUESTION)
        .output()
        .expect("run cru chat");

    stop_daemon_and_wait(&daemon);

    OneShotRun {
        sessions: daemon.sessions_root(),
        output,
        _daemon: daemon,
        _workspace: workspace,
    }
}

/// Stop the daemon and block until its process exits. After this, every
/// session file is complete.
///
/// `cru chat` exiting does not mean the daemon finished writing. The persist
/// task (`server/mod.rs`) appends `session.jsonl` off the daemon's broadcast
/// channel, and on the first event of a session it also rewrites `meta.json`
/// through `update_last_activity`. That rewrite truncates the file before it
/// writes, so a reader can see an empty `meta.json` — which is what failed on
/// CI, as a parse error. Both writes happen after the client already has its
/// reply. A poll with a wall-clock deadline was the previous answer; it moved
/// the failure to a slower machine instead of removing it.
///
/// A graceful shutdown is the one ordered signal: `Server::run` drains the
/// persist task before it returns, and the process exits behind it. The test
/// cannot wait on the daemon's `Child` — `TestDaemon` keeps it private — so it
/// holds an idle connection instead. The daemon never closes an idle client on
/// its own; the connection closes when the process exits. EOF here is the exit
/// itself, and no clock is involved.
fn stop_daemon_and_wait(daemon: &TestDaemon) {
    let mut sentinel =
        UnixStream::connect(&daemon.socket_path).expect("open a sentinel connection to the daemon");
    let stop = daemon
        .command()
        .args(["daemon", "stop"])
        .output()
        .expect("run cru daemon stop");
    let stdout = String::from_utf8_lossy(&stop.stdout);
    // `cru daemon stop` exits 0 when it finds no daemon. Without this check
    // that case reads as "stopped" and the wait below never returns.
    assert!(
        stop.status.success() && stdout.contains("Daemon stopped"),
        "cru daemon stop did not reach the daemon. exit {:?}, stdout:\n{stdout}\nstderr:\n{}",
        stop.status.code(),
        String::from_utf8_lossy(&stop.stderr)
    );
    let mut discard = Vec::new();
    sentinel
        .read_to_end(&mut discard)
        .expect("read the sentinel connection to EOF");
}

/// The single session directory the run created. Each run gets a fresh daemon
/// and kiln, so "the only one" is well defined — no timestamp guessing.
///
/// Read once: the daemon already exited (see [`stop_daemon_and_wait`]), so a
/// missing directory is a real failure, not an early read.
fn sole_session_dir(run: &OneShotRun) -> PathBuf {
    let sessions = &run.sessions;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(sessions)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("session.jsonl").is_file())
                .collect()
        })
        .unwrap_or_default();
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
    let run = run_one_shot(&["--card", "researcher"]);
    let session_dir = sole_session_dir(&run);
    let agent = read_json(&session_dir.join("meta.json"))["agent"].clone();
    assert_eq!(agent["agent_card_name"], "researcher");
    assert_eq!(agent["system_prompt"], "Keep the kiln authoritative.");
    let events = transcript(&session_dir);

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
}
