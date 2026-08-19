//! Session titling, end to end through a scripted titling plugin.
//!
//! The subject is the seam the `auto-title` port created: the daemon asks
//! whoever publishes `session_title` and persists what comes back, and
//! truncates the first user message when nothing does. The plugin here is
//! scripted rather than the bundled one — its own suite
//! (`runtime/plugins/auto-title/tests/`) covers the prompt, the clip and the
//! sanitizer, and a test that needed a real provider would prove neither.

use super::create_test_agent_manager;
use crate::agent_manager::AgentManager;
use crate::observe::events::LogEvent;
use crate::session_manager::SessionManager;
use crucible_core::session::SessionType;
use crucible_lua::{DiscoveredCommand, PublicationRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// A session whose log holds one exchange, which is what titling reads.
async fn session_with_an_opening_exchange(sm: &Arc<SessionManager>, user: &str) -> String {
    let session = sm
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .expect("create session");
    let path = session.jsonl_path(sm.sessions_root());
    tokio::fs::create_dir_all(path.parent().expect("session dir"))
        .await
        .expect("create session dir");
    let lines = [LogEvent::user(user), LogEvent::assistant("of course")]
        .iter()
        .map(|e| e.to_jsonl().expect("serialize"))
        .collect::<Vec<_>>()
        .join("\n");
    tokio::fs::write(&path, lines + "\n")
        .await
        .expect("write session log");
    session.id.to_string()
}

/// Register `command` on a manager as if a plugin had declared it, and publish
/// `declaration` on the `session_title` channel under `plugin`.
///
/// Goes through the real [`crate::plugin_tools::PluginRegistry`] and the real
/// [`PublicationRegistry`] — the two things the daemon actually reads — so the
/// only fake here is the Lua body.
fn install_titler(
    am: &AgentManager,
    plugin: &str,
    command: &str,
    declaration: serde_json::Value,
    body: &'static str,
) {
    let lua = mlua::Lua::new();
    let func = lua
        .load(body)
        .eval::<mlua::Function>()
        .expect("the scripted command compiles");

    let registry = crate::plugin_tools::PluginRegistry::new();
    registry.register_plugin(
        plugin,
        &lua,
        &[],
        &[DiscoveredCommand {
            name: command.to_string(),
            description: "scripted titler".to_string(),
            params: Vec::new(),
            input_hint: None,
            source_path: "test".to_string(),
            handler_fn: "fn".to_string(),
            is_fennel: false,
        }],
        HashMap::new(),
        HashMap::from([(command.to_string(), func)]),
    );
    am.set_plugin_tool_registry(Arc::new(registry));

    let publications = PublicationRegistry::new();
    publications.set(plugin, "session_title", declaration);
    am.set_publications(publications);
}

/// The port's whole point: the title comes from Lua, and the daemon persists
/// it and announces it.
#[tokio::test]
async fn a_publishing_plugin_titles_the_session() {
    let sm = crate::test_support::temp_session_manager();
    let id = session_with_an_opening_exchange(&sm, "please help me fix the auth flow").await;
    let am = create_test_agent_manager(sm.clone());
    install_titler(
        &am,
        "scripted-titler",
        "scripted.title",
        serde_json::json!({ "command": "scripted.title" }),
        "return function(args) return { title = 'Fixing the auth flow' } end",
    );

    let (tx, mut rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");

    assert_eq!(title, "Fixing the auth flow");
    assert_eq!(
        sm.get_session(&id).and_then(|s| s.title),
        Some("Fixing the auth flow".to_string()),
        "the plugin's title must be persisted, not merely returned"
    );
    let event = rx.try_recv().expect("a title_changed event");
    assert_eq!(event.event, "title_changed");
    assert_eq!(event.data["title"], "Fixing the auth flow");
}

/// The opening exchange reaches the plugin — without it the plugin has
/// nothing to title from, and the clip and the prompt it owns are pointless.
#[tokio::test]
async fn the_plugin_is_handed_the_opening_exchange() {
    let sm = crate::test_support::temp_session_manager();
    let id = session_with_an_opening_exchange(&sm, "help me fix the auth flow").await;
    let am = create_test_agent_manager(sm.clone());
    install_titler(
        &am,
        "scripted-titler",
        "scripted.title",
        serde_json::json!({ "command": "scripted.title" }),
        "return function(args) return { title = args.user .. ' / ' .. args.assistant } end",
    );

    let (tx, _rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");
    assert_eq!(title, "help me fix the auth flow / of course");
}

/// No plugin at all — the shipped state of a daemon with `auto-title`
/// disabled, and of every test manager. A session with content must still get
/// a title.
#[tokio::test]
async fn no_titling_plugin_falls_back_to_truncation() {
    let sm = crate::test_support::temp_session_manager();
    let id = session_with_an_opening_exchange(&sm, "please help me fix the auth flow").await;
    let am = create_test_agent_manager(sm.clone());

    let (tx, _rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");
    assert_eq!(title, "please help me fix the auth flow");
}

/// A raising plugin is the failed-completion path: no API key, a provider
/// error, a timeout. The daemon owns the fallback, so the session is titled
/// anyway.
#[tokio::test]
async fn a_raising_plugin_falls_back_to_truncation() {
    let sm = crate::test_support::temp_session_manager();
    let id = session_with_an_opening_exchange(&sm, "please help me fix the auth flow").await;
    let am = create_test_agent_manager(sm.clone());
    install_titler(
        &am,
        "scripted-titler",
        "scripted.title",
        serde_json::json!({ "command": "scripted.title" }),
        "return function(args) error('no API key') end",
    );

    let (tx, _rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");
    assert_eq!(title, "please help me fix the auth flow");
}

/// An empty answer is the one case the plugin's sanitizer cannot rescue: a
/// model that replied with whitespace. An empty title is worse than a
/// truncated one.
#[tokio::test]
async fn an_empty_plugin_answer_falls_back_to_truncation() {
    let sm = crate::test_support::temp_session_manager();
    let id = session_with_an_opening_exchange(&sm, "please help me fix the auth flow").await;
    let am = create_test_agent_manager(sm.clone());
    install_titler(
        &am,
        "scripted-titler",
        "scripted.title",
        serde_json::json!({ "command": "scripted.title" }),
        "return function(args) return { title = '   ' } end",
    );

    let (tx, _rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");
    assert_eq!(title, "please help me fix the auth flow");
}

/// A publication naming a command nobody declares must not take the session
/// down with it — it is a plugin bug, and the fallback is still a title.
#[tokio::test]
async fn a_publication_naming_an_undeclared_command_falls_back() {
    let sm = crate::test_support::temp_session_manager();
    let id = session_with_an_opening_exchange(&sm, "please help me fix the auth flow").await;
    let am = create_test_agent_manager(sm.clone());
    install_titler(
        &am,
        "scripted-titler",
        "scripted.title",
        serde_json::json!({ "command": "scripted.absent" }),
        "return function(args) return { title = 'never reached' } end",
    );

    let (tx, _rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");
    assert_eq!(title, "please help me fix the auth flow");
}

/// The two halves of the port meet: the bundled plugin publishes the channel
/// this file's constant names, declares the command it published, and reaches
/// `cru.sessions.complete` when the command runs.
///
/// Everything above scripts the plugin; this loads the real one through the
/// real loader. Without it the daemon could look up `session_title` while the
/// plugin published `title`, and every test would still pass with sessions
/// quietly falling back to truncation forever.
#[tokio::test]
async fn the_bundled_plugin_publishes_the_channel_the_daemon_reads() {
    let shipped = std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtime/plugins"
    ));
    let mut loader =
        crate::daemon_plugins::DaemonPluginLoader::new(HashMap::new()).expect("plugin loader");
    loader
        .load_plugins(&[(shipped, crucible_lua::PluginSource::Runtime)])
        .await
        .expect("load shipped plugins");

    let published = loader.publications().get("session_title");
    let (plugin, declaration) = published
        .first()
        .expect("a shipped plugin must publish 'session_title'");
    assert_eq!(plugin, "auto-title");
    let command = declaration["command"]
        .as_str()
        .expect("the declaration names a command");

    // Called with no daemon behind the sessions module, so `complete` answers
    // `(nil, "no daemon connected")` and the plugin raises with that reason.
    // A missing binding would raise "attempt to call a nil value" instead,
    // which is the failure this half of the assertion exists to catch.
    let err = loader
        .plugin_registry()
        .run_command(
            command,
            serde_json::json!({ "session_id": "chat-1", "user": "hello" }),
        )
        .await
        .expect_err("no daemon is connected, so the completion cannot succeed")
        .to_string();
    assert!(
        err.contains("no daemon connected"),
        "the command must reach cru.sessions.complete; got: {err}"
    );
}

/// A session titled after its first turn has no assistant reply yet. The
/// plugin must see that as `nil` — a JSON null crosses into Lua as a
/// lightuserdata, which is truthy, so `if args.assistant then` would lie.
#[tokio::test]
async fn an_absent_assistant_turn_reaches_the_plugin_as_nil() {
    let sm = crate::test_support::temp_session_manager();
    let session = sm
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .expect("create session");
    let path = session.jsonl_path(sm.sessions_root());
    tokio::fs::create_dir_all(path.parent().expect("session dir"))
        .await
        .expect("create session dir");
    tokio::fs::write(
        &path,
        LogEvent::user("just the one turn")
            .to_jsonl()
            .expect("serialize")
            + "\n",
    )
    .await
    .expect("write session log");
    let id = session.id.to_string();

    let am = create_test_agent_manager(sm.clone());
    install_titler(
        &am,
        "scripted-titler",
        "scripted.title",
        serde_json::json!({ "command": "scripted.title" }),
        "return function(args) return { title = type(args.assistant) } end",
    );

    let (tx, _rx) = broadcast::channel(8);
    let title = am.generate_session_title(&id, &tx).await.expect("a title");
    assert_eq!(title, "nil");
}
