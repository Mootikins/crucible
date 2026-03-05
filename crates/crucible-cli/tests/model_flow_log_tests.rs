//! Captures tracing events from the `model_flow` target and asserts on state transitions.

use std::io;
use std::sync::{Arc, Mutex};

use crucible_cli::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crucible_cli::tui::oil::{App, AppHarness};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

impl io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::MakeWriter<'a> for SharedBuf {
    type Writer = SharedBuf;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn model_flow_subscriber(buf: SharedBuf) -> impl tracing::Subscriber {
    tracing_subscriber::registry()
        .with(EnvFilter::new("crucible_cli::tui::oil::model_flow=debug"))
        .with(
            fmt::layer()
                .with_writer(buf)
                .with_target(true)
                .with_level(true)
                .with_ansi(false)
                .without_time(),
        )
}

#[test]
fn model_flow_log_state_transitions_to_loaded() {
    let buf = SharedBuf::new();
    let subscriber = model_flow_subscriber(buf.clone());

    tracing::subscriber::with_default(subscriber, || {
        let mut app = OilChatApp::init();
        app.on_message(ChatAppMsg::FetchModels);
        app.on_message(ChatAppMsg::ModelsLoaded(vec!["ollama/llama3".to_string()]));
    });

    let output = buf.output();
    assert!(
        output.contains("FetchModels") && output.contains("Loading"),
        "Should log FetchModels -> Loading transition.\nCaptured:\n{output}"
    );
    assert!(
        output.contains("ModelsLoaded") && output.contains("Loaded"),
        "Should log ModelsLoaded -> Loaded transition.\nCaptured:\n{output}"
    );
    assert!(
        output.contains("count"),
        "Should log model count field.\nCaptured:\n{output}"
    );
}

#[test]
fn model_flow_log_state_transitions_to_failed() {
    let buf = SharedBuf::new();
    let subscriber = model_flow_subscriber(buf.clone());

    tracing::subscriber::with_default(subscriber, || {
        let mut app = OilChatApp::init();
        app.on_message(ChatAppMsg::FetchModels);
        app.on_message(ChatAppMsg::ModelsFetchFailed(
            "connection refused".to_string(),
        ));
    });

    let output = buf.output();
    assert!(
        output.contains("FetchModels") && output.contains("Loading"),
        "Should log FetchModels -> Loading transition.\nCaptured:\n{output}"
    );
    assert!(
        output.contains("ModelsFetchFailed") && output.contains("Failed"),
        "Should log ModelsFetchFailed -> Failed transition.\nCaptured:\n{output}"
    );
    assert!(
        output.contains("connection refused"),
        "Should log the error reason.\nCaptured:\n{output}"
    );
}

#[test]
fn model_flow_log_repl_command_state() {
    let buf = SharedBuf::new();
    let subscriber = model_flow_subscriber(buf.clone());

    tracing::subscriber::with_default(subscriber, || {
        let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
        harness.render();

        harness.send_text(":model");
        harness.send_enter();
    });

    let output = buf.output();
    assert!(
        output.contains("handle_repl_command") && output.contains("model pressed"),
        "Should log handle_repl_command: model pressed.\nCaptured:\n{output}"
    );
    assert!(
        output.contains("NotLoaded"),
        "Should log state=NotLoaded for initial model command.\nCaptured:\n{output}"
    );
}

#[test]
fn model_flow_log_full_lifecycle() {
    let buf = SharedBuf::new();
    let subscriber = model_flow_subscriber(buf.clone());

    tracing::subscriber::with_default(subscriber, || {
        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::FetchModels);
        app.on_message(ChatAppMsg::ModelsFetchFailed("timeout".to_string()));

        app.on_message(ChatAppMsg::FetchModels);
        app.on_message(ChatAppMsg::ModelsLoaded(vec![
            "ollama/llama3".to_string(),
            "openai/gpt-4".to_string(),
        ]));
    });

    let output = buf.output();

    assert!(
        output.contains("timeout"),
        "Should log first failure reason.\nCaptured:\n{output}"
    );
    assert!(
        output.contains("ModelsLoaded"),
        "Should log eventual success.\nCaptured:\n{output}"
    );

    let fetch_count = output.matches("FetchModels").count();
    assert!(
        fetch_count >= 2,
        "Should log FetchModels at least twice (got {fetch_count}).\nCaptured:\n{output}"
    );
}
