//! Which screen the real `cru chat` binary draws on (US-804).
//!
//! The mode is chosen in the process, from the flags and the config, so only
//! a spawned binary proves it. vt100 records whether the child switched to
//! the alternate screen (`?1049h`) and whether it asked for mouse reports.

use std::time::Duration;

use super::tui_e2e_harness::{TuiTestConfig, TuiTestSession};

fn spawn_ready(config: TuiTestConfig) -> TuiTestSession {
    let mut session = TuiTestSession::spawn(config.with_env("RUST_LOG", "warn")).expect("spawn");
    session.wait_for_ready().expect("TUI ready");
    session
}

/// Quit, then prove that the child gave the main screen back.
fn quit_to_main_screen(mut session: TuiTestSession) {
    session.send(":quit\r").expect("quit");
    session
        .wait_until(|s| !s.alternate_screen(), Duration::from_secs(5))
        .expect("the TUI leaves the alternate screen on exit");
    session.expect_eof().expect("runner exited");
}

#[test]
#[ignore = "requires: cru binary"]
fn a_plain_cru_chat_draws_on_the_alternate_screen() {
    let session = spawn_ready(TuiTestConfig::new("chat"));

    assert!(
        session.screen().alternate_screen(),
        "a plain `cru chat` must draw full screen:\n{}",
        session.screen_contents()
    );
    assert_ne!(
        session.screen().mouse_protocol_mode(),
        vt100::MouseProtocolMode::None,
        "the full-screen mode asks for mouse reports to select text"
    );

    quit_to_main_screen(session);
}

#[test]
#[ignore = "requires: cru binary"]
fn cru_chat_inline_stays_on_the_main_screen() {
    let mut session = spawn_ready(TuiTestConfig::new("chat").with_args(&["--inline"]));

    assert!(
        !session.screen().alternate_screen(),
        "`--inline` must keep the chat on the main screen:\n{}",
        session.screen_contents()
    );
    assert_eq!(
        session.screen().mouse_protocol_mode(),
        vt100::MouseProtocolMode::None,
        "the inline mode leaves the mouse to the terminal"
    );

    session.send(":quit\r").expect("quit");
    session.expect_eof().expect("runner exited");
}

#[test]
#[ignore = "requires: cru binary"]
fn the_cli_screen_setting_selects_the_inline_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("init.lua");
    // The harness's own config gives the provider; `--config` replaces it,
    // so this file names the provider too.
    std::fs::write(
        &config_path,
        "cru.config.set({ cli = { screen = \"inline\" }, llm = { default = \"ollama\", providers = { ollama = { type = \"ollama\" } } } })\n",
    )
    .expect("write config");
    let path = config_path.display().to_string();

    let mut session = spawn_ready(TuiTestConfig::new("chat").with_args(&["--config", &path]));

    assert!(
        !session.screen().alternate_screen(),
        "`cli.screen = \"inline\"` must keep the chat on the main screen:\n{}",
        session.screen_contents()
    );

    session.send(":quit\r").expect("quit");
    session.expect_eof().expect("runner exited");
}

/// The first-run wizard asks its questions on the main screen, before the
/// chat TUI takes the terminal. A config directory with no `init.lua` makes
/// the run a first run.
#[test]
#[ignore = "requires: cru binary"]
fn the_first_run_wizard_prompts_on_the_main_screen() {
    let empty_config = tempfile::tempdir().expect("tempdir");
    let dir = empty_config.path().display().to_string();
    let mut session = TuiTestSession::spawn(
        TuiTestConfig::new("chat")
            .with_env("RUST_LOG", "warn")
            .with_env("CRUCIBLE_CONFIG_DIR", &dir),
    )
    .expect("spawn");

    session
        .wait_for_text("LLM Provider", Duration::from_secs(5))
        .expect("the first-run wizard asks for a provider");
    assert!(
        !session.screen().alternate_screen(),
        "the wizard must prompt on the main screen:\n{}",
        session.screen_contents()
    );
    assert_eq!(
        session.screen().mouse_protocol_mode(),
        vt100::MouseProtocolMode::None,
        "the wizard leaves the mouse to the terminal"
    );
}
