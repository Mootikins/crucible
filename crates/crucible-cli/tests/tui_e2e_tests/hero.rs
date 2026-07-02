//! Hero-flow TUI legs (legs 1 and 3 of the cross-surface hero journey).
//!
//! These are NOT standalone: they are driven by the Playwright hero spec
//! (`crates/crucible-cli/web/e2e/live/hero.live.spec.ts`), which stands up an
//! isolated daemon + `cru web` + a fake Ollama server, then invokes each leg via
//! `cargo nextest run --run-ignored ignored-only -E 'test(hero_leg_N)'` with the
//! daemon's env. The legs attach to that SAME daemon (a session is a VM on the
//! hypervisor; the TUI is one console) and hand off via a JSON state file.
//!
//! Required env (set by the Playwright harness):
//!   HERO_STATE     — path to the JSON handoff file { session_id, kiln }
//!   HERO_KILN      — kiln directory (working dir for cru + shell writes)
//!   HERO_ARTIFACT  — dir to dump captured TUI frames into (image-sequence parity)
//!   CRUCIBLE_SOCKET, CRUCIBLE_CONFIG_DIR, HOME, XDG_* — daemon env (inherited)
//!   CRU_BIN        — the cru binary to use (defaults to workspace target/debug/cru)
//!
//! Without HERO_STATE the legs skip (so a plain `--ignored` sweep is a no-op).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use super::tui_e2e_harness::{Key, TuiTestConfig, TuiTestSession};

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            eprintln!("SKIPPED hero leg: {key} not set (run via the Playwright hero harness)");
            None
        }
    }
}

/// The cru binary the harness built (CRU_BIN), else the workspace debug build.
fn cru_bin() -> PathBuf {
    if let Ok(p) = std::env::var("CRU_BIN") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("target/debug/cru")
}

/// Run `cru <args>` in the kiln dir with inherited env; return trimmed stdout.
fn run_cru(kiln: &str, args: &[&str]) -> String {
    let out = Command::new(cru_bin())
        .args(args)
        .current_dir(kiln)
        .output()
        .unwrap_or_else(|e| panic!("failed to run cru {args:?}: {e}"));
    assert!(
        out.status.success(),
        "cru {args:?} failed: status={:?}\nstdout={}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A TuiTestConfig for `cru chat --resume <id>`, forcing the harness binary and
/// re-exporting the daemon env so the child attaches to the same daemon.
fn chat_resume_config(session_id: &str, kiln: &str) -> TuiTestConfig {
    // cwd = the kiln: `cru chat --resume` derives config.kiln_path from the cwd,
    // and fetch_resume_history queries that kiln — so history only hydrates when
    // the process runs inside the session's kiln.
    let mut config = TuiTestConfig::new("chat")
        .with_args(&["--resume", session_id])
        .with_dimensions(110, 40)
        .with_cwd(kiln)
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(90));
    config.binary_path = Some(cru_bin());
    // Re-export the daemon env explicitly (expectrl inherits parent env too, but
    // be explicit so a leg is reproducible from its command line).
    for key in ["CRUCIBLE_SOCKET", "CRUCIBLE_CONFIG_DIR", "HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"] {
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                config = config.with_env(key, &v);
            }
        }
    }
    config
}

fn dump_frame(session: &mut TuiTestSession, artifact_dir: &str, name: &str) {
    session.refresh_screen();
    let contents = session.screen_contents();
    let path = PathBuf::from(artifact_dir).join(format!("{name}.txt"));
    let _ = fs::create_dir_all(artifact_dir);
    let _ = fs::write(&path, contents);
}

/// Poll a file until it contains `needle`, or panic with the last content.
fn wait_for_file_contains(path: &str, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let last = fs::read_to_string(path).unwrap_or_default();
        if last.contains(needle) {
            return;
        }
        if Instant::now() > deadline {
            panic!("file {path} never contained {needle:?}; last content = {last:?}");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Leg 1 — the TUI console opens the session, drives turn 1, and writes a file
/// via the shell modal. Proves: a terminal starts real work on the daemon and
/// touches a kiln buffer that the next console will see.
#[test]
#[ignore = "hero flow — driven by the Playwright live harness (needs HERO_STATE env)"]
fn hero_leg_1() {
    let Some(hero_state) = env_or_skip("HERO_STATE") else { return };
    let Some(kiln) = env_or_skip("HERO_KILN") else { return };
    let artifact = std::env::var("HERO_ARTIFACT").unwrap_or_else(|_| kiln.clone());

    // Create the session on the daemon and capture its id (`-q` prints only the
    // id). This is the "VM" every console will later attach to.
    let session_id = run_cru(&kiln, &["session", "create", "-q"]);
    assert!(!session_id.is_empty(), "cru session create -q returned empty id");

    // Publish the handoff state for legs 2 and 3.
    let state = format!(
        "{{\n  \"session_id\": \"{session_id}\",\n  \"kiln\": \"{kiln}\"\n}}\n"
    );
    fs::write(&hero_state, state).expect("write HERO_STATE");

    let mut session = TuiTestSession::spawn(chat_resume_config(&session_id, &kiln)).expect("spawn cru chat");
    session.wait_for_ready().expect("TUI never reached NORMAL");
    dump_frame(&mut session, &artifact, "leg1-01-ready");

    // Turn 1: a scripted prompt. "baseline" is in the fake reply, NOT the prompt,
    // so a hit proves the daemon streamed the model turn back to this console.
    session.send("Summarize [[Seed]]").expect("send prompt");
    session.send_key(Key::Enter).expect("enter");
    session
        .wait_until(|s| s.contents().to_lowercase().contains("baseline"), Duration::from_secs(60))
        .expect("turn 1 reply never rendered in the TUI");
    dump_frame(&mut session, &artifact, "leg1-02-turn1");

    // Shell modal: write a new note through the terminal (absolute path, so cwd
    // is irrelevant). This is the buffer the web console will edit next.
    let note = format!("{kiln}/notes/from-tui.md");
    session
        .send(&format!("!printf 'terminal was here' > {note}"))
        .expect("send shell command");
    session.wait_for_text("from-tui.md", Duration::from_secs(5)).ok();
    dump_frame(&mut session, &artifact, "leg1-03a-shell-typed");
    session.send_key(Key::Enter).expect("enter");
    std::thread::sleep(Duration::from_millis(500));
    dump_frame(&mut session, &artifact, "leg1-03b-shell-submitted");
    wait_for_file_contains(&note, "terminal was here", Duration::from_secs(15));
    dump_frame(&mut session, &artifact, "leg1-03c-shell-wrote-file");

    // Detach: Ctrl-C exits the TUI without typing a stray message (`:quit` races
    // the Enter and can submit a bogus turn), then pause so the session is
    // resumable-from-storage by the next console (history hydration requires the
    // session to be non-active).
    detach(&mut session, &kiln, &session_id);
}

/// Leg 3 — a fresh TUI console re-attaches to the same session and proves the
/// hypervisor holds all the state: turns 1 AND 2 (the web-sent turn) hydrate,
/// and `!cat` shows the BROWSER's edit to the shared buffer.
#[test]
#[ignore = "hero flow — driven by the Playwright live harness (needs HERO_STATE env)"]
fn hero_leg_3() {
    let Some(hero_state) = env_or_skip("HERO_STATE") else { return };
    let Some(kiln) = env_or_skip("HERO_KILN") else { return };
    let artifact = std::env::var("HERO_ARTIFACT").unwrap_or_else(|_| kiln.clone());

    let state = fs::read_to_string(&hero_state).expect("read HERO_STATE");
    let session_id = parse_json_string(&state, "session_id").expect("session_id in HERO_STATE");

    let mut session = TuiTestSession::spawn(chat_resume_config(&session_id, &kiln)).expect("spawn cru chat");
    session.wait_for_ready().expect("TUI never reached NORMAL");

    // Both turns hydrate from daemon history: turn 1 ("baseline") was sent from
    // the TUI, turn 2 ("records") from the browser — yet both are visible here.
    session
        .wait_until(
            |s| {
                let c = s.contents().to_lowercase();
                c.contains("baseline") && c.contains("records")
            },
            Duration::from_secs(30),
        )
        .expect("turns 1 and 2 did not both hydrate in the terminal");
    dump_frame(&mut session, &artifact, "leg3-01-hydrated-both-turns");

    // `!cat` shows the BROWSER's edit ("browser was here") to the shared buffer.
    let note = format!("{kiln}/notes/from-tui.md");
    session.send(&format!("!cat {note}")).expect("send cat");
    // Settle: ensure the full command is typed before submitting (a bare Enter
    // races the long path and would submit a truncated command).
    session.wait_for_text("from-tui.md", Duration::from_secs(5)).ok();
    session.send_key(Key::Enter).expect("enter");
    session
        .wait_until(|s| s.contents().contains("browser was here"), Duration::from_secs(15))
        .expect("browser edit not visible in the terminal shell output");
    dump_frame(&mut session, &artifact, "leg3-02-cat-shows-browser-edit");

    // Close the shell-output modal (`q` quits) so input returns to the chat
    // composer, then wait for the NORMAL prompt.
    session.send("q").ok();
    session
        .wait_until(|s| s.contents().contains("NORMAL"), Duration::from_secs(10))
        .expect("shell modal did not close back to NORMAL");

    // Turn 3 from the terminal, to confirm the session is still live for writes.
    session.send("Please confirm the note contents.").expect("send turn 3");
    session.wait_for_text("confirm the note", Duration::from_secs(5)).ok();
    session.send_key(Key::Enter).expect("enter");
    session
        .wait_until(|s| s.contents().to_lowercase().contains("carries"), Duration::from_secs(60))
        .expect("turn 3 reply never rendered");
    dump_frame(&mut session, &artifact, "leg3-03-turn3");

    detach(&mut session, &kiln, &session_id);
}

/// Ctrl-C out of the TUI, then pause the session so the next console can resume
/// it from storage (history hydration and `--resume` need a non-active session).
fn detach(session: &mut TuiTestSession, kiln: &str, session_id: &str) {
    let _ = session.send_control('c');
    let _ = session.send_control('c');
    session.settle();
    let _ = Command::new(cru_bin())
        .args(["session", "pause", session_id])
        .current_dir(kiln)
        .output();
}

/// Minimal string-field extractor for our own two-field handoff JSON.
fn parse_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let colon = rest.find(':')?;
    let after = &rest[colon + 1..];
    let q1 = after.find('"')? + 1;
    let q2 = after[q1..].find('"')? + q1;
    Some(after[q1..q2].to_string())
}
