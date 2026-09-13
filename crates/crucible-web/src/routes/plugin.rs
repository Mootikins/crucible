use crate::routes::plugin_caller::PluginCaller;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use crucible_core::protocol::SystemPayload;
use crucible_daemon::server::plugins::OptionAction;
use crucible_daemon::SessionEvent;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::StreamExt;

/// The ten plugin endpoints, six of which take a caller identity and four of
/// which do not.
///
/// Gated by [`PluginCaller`]: `POST /api/plugins` (install), `DELETE
/// /api/plugins/{name}`, `POST /api/plugins/{name}/reload` — app only; `POST
/// /api/plugins/{name}/option` and `POST /api/plugins/command` — the app, or
/// the plugin that owns the thing; `GET /api/plugins/publications` — narrowed
/// to the caller's own.
///
/// Ungated, and each for a reason worth knowing before you "finish the job":
/// `GET /api/plugins`, `/commands` and `/options` are enumerations the plugins
/// panel needs and no route rewrites, so gating them buys nothing while a
/// block can call itself `app`. `GET /api/plugins/events` **cannot** be gated
/// this way at all: browsers open it with `EventSource`, which sets no
/// headers. An identity for the push stream needs a different carrier.
/// **No route here serves a plugin's own web assets, and adding one has a
/// precondition.**
///
/// A plugin's JavaScript would run on the app origin, where `script-src
/// 'self'` means what `web/src/pwa-options.ts` says it means, and where a
/// block can already call any endpoint as `app`. The decision on record is a
/// sandboxed iframe on an opaque origin with a `MessageChannel` bridge; the
/// evidence, the measurements and the envelope are in
/// `docs/Meta/Analysis/Plugin Web Delivery.md`.
///
/// The trigger is a third-party plugin that ships web assets. Build the
/// bridge first, then the route.
///
/// This note sits here rather than as a refusal in the install path, and the
/// reason is worth stating: a refusal keyed on a `web/` directory rejects a
/// plugin that keeps unrelated sources under that name, and `cru.rtp` lets a
/// plugin arrive through a runtime root without passing the install path at
/// all. A gate that is wrong in both directions is worse than a line the
/// person adding the route will read.
pub fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/plugins", get(list_plugins).post(install_plugin))
        .route("/api/plugins/{name}", delete(remove_plugin))
        .route("/api/plugins/{name}/reload", post(reload_plugin))
        .route("/api/plugins/publications", get(list_publications))
        .route("/api/plugins/commands", get(list_commands))
        .route("/api/plugins/options", get(list_options))
        .route("/api/plugins/{name}/option", post(option_call))
        .route("/api/plugins/events", get(publication_event_stream))
        .route("/api/plugins/command", post(run_command))
}

/// One plugin command invocation.
#[derive(Debug, Deserialize)]
struct CommandRequest {
    name: String,
    #[serde(default)]
    args: serde_json::Value,
}

/// One read, write, or button press against a plugin's settings tree.
///
/// A single endpoint because the three differ only in which Lua callback they
/// reach — the same reason the daemon handler is one function. `value` is
/// present for a set and ignored otherwise.
#[derive(Debug, Deserialize)]
struct OptionRequest {
    action: OptionAction,
    path: Vec<String>,
    #[serde(default)]
    value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct InstallRequest {
    /// Plugin URL (e.g. "user/repo" or full git URL).
    url: String,
    branch: Option<String>,
    pin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RemoveQuery {
    #[serde(default)]
    purge: bool,
}

/// `GET /api/plugins` — list discovered plugins with rich metadata
/// (name, version, source, state, dir, capability counts).
async fn list_plugins(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let info = state.daemon.plugin_list_info().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "plugins": info })))
}

/// `GET /api/plugins/publications` — what plugins published about themselves.
///
/// Passed through verbatim, keyed `key -> plugin -> value`. Nothing here
/// interprets a value, which is the point: the frontend used to learn what
/// isolation a box offered by having the server match on the shape of the `oci`
/// plugin's config, so one plugin's schema lived in the rendering layer and a
/// second isolating plugin would not have appeared at all.
async fn list_publications(
    State(state): State<AppState>,
    caller: PluginCaller,
    Query(q): Query<PublicationsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let publications = state.daemon.plugin_publications(q.key).await.daemon_err()?;
    Ok(Json(
        serde_json::json!({ "publications": narrow_to_caller(publications, &caller) }),
    ))
}

/// Keep only what the caller may see: everything for the app, a plugin's own
/// rows for a plugin.
///
/// Narrowing rather than refusing, because `?key=` is a courtesy and this is
/// the boundary the courtesy stands in for. The answer is keyed `key -> plugin
/// -> value`, so a plugin's own rows are the inner entries under its name; a
/// key left with nothing under it is dropped rather than answered as empty.
///
/// `PluginBlockPanel` reads every plugin's publications with no key and is the
/// app, so it passes through here untouched. Narrow the app too and that panel
/// goes blank.
fn narrow_to_caller(publications: serde_json::Value, caller: &PluginCaller) -> serde_json::Value {
    let PluginCaller::Plugin(plugin) = caller else {
        return publications;
    };
    let serde_json::Value::Object(keys) = publications else {
        return publications;
    };
    let mine = keys
        .into_iter()
        .filter_map(|(key, by_plugin)| {
            let value = by_plugin.get(plugin.as_str())?.clone();
            Some((key, serde_json::json!({ plugin.as_str(): value })))
        })
        .collect::<serde_json::Map<_, _>>();
    serde_json::Value::Object(mine)
}

/// `GET /api/plugins/commands` — the executable primitives plugins declared.
///
/// Passed through verbatim. Each entry carries `plugin`, `name`,
/// `description`, `hint` and `parameters`, and nothing here reads any of them:
/// a caller offering a command as a button, with a dialog built from its
/// declared parameters, needs no change on this side when a plugin ships a new
/// one.
///
/// `parameters` crosses as opaque JSON today. It comes from the same
/// `ToolDefinition` a tool uses, so shaping it like `signature.rs`'s JSON
/// Schema output is what would let a dialog be generated rather than
/// hand-read — see `docs/Meta/Analysis/The Plugin Contract.md`.
async fn list_commands(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let commands = state.daemon.plugin_commands().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "commands": commands })))
}

/// `?key=` narrows to one contribution kind.
///
/// Without it every caller receives every plugin's published data. That is
/// more than a block drawing one key needs, and — once third-party block code
/// can run — more than it should be handed.
#[derive(Debug, Deserialize)]
struct PublicationsQuery {
    #[serde(default)]
    key: Option<String>,
}

/// `GET /api/plugins/options` — the settings trees plugins declared.
///
/// Passed through verbatim, keyed by plugin. Nothing here knows what any
/// option means: a node is `{type, name, desc, order, …}` and the renderer
/// draws it from that, so a plugin shipped tomorrow gets a settings pane with
/// no change on this side. Re-read rather than cached — a tree's
/// function-valued fields describe the box as it is now.
async fn list_options(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let options = state.daemon.plugin_options().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "options": options })))
}

/// `POST /api/plugins/:name/option` — read, write, or press one option.
async fn option_call(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
    Json(req): Json<OptionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // `{name}` is caller-supplied, so without this a block reads and writes
    // any plugin's settings tree by naming it.
    caller.require_speaks_for(&name, "read or write the settings")?;
    if req.path.is_empty() {
        return Err(WebError::Validation(
            "`path` must name an option".to_string(),
        ));
    }
    match req.action {
        OptionAction::Get => {
            let value = state
                .daemon
                .plugin_option_get(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(serde_json::json!({ "value": value })))
        }
        OptionAction::Set => {
            state
                .daemon
                .plugin_option_set(&name, req.path, req.value)
                .await
                .daemon_err()?;
            Ok(Json(serde_json::json!({ "ok": true })))
        }
        OptionAction::Execute => {
            state
                .daemon
                .plugin_option_execute(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(serde_json::json!({ "ok": true })))
        }
    }
}

/// A plugin's published data changed, delivered to the browser.
///
/// Carries who published and under which key, never the value — the same
/// contract the daemon event has, and for the same reason: a publication is
/// opaque JSON of the plugin's own choosing. The browser refetches through
/// `GET /api/plugins/publications`.
///
/// Projected through a named type rather than passed through as raw `data`,
/// which is the treatment `SurfaceChangedEvent` already gets: a field the
/// daemon renames then breaks the browser with nothing on this side to notice.
#[derive(Debug, Clone, Serialize)]
pub struct PublicationChangedEvent {
    pub plugin: String,
    pub key: String,
}

impl PublicationChangedEvent {
    /// The SSE `event:` name the browser listens for.
    ///
    /// Read from the daemon's own payload, never written again here: this file
    /// once held a literal of its own, and the name it had to match lived two
    /// crates away with nothing comparing them.
    pub const EVENT_NAME: &'static str = SystemPayload::PUBLICATION_CHANGED;

    /// Project a daemon event into this shape, or `None` for anything else.
    ///
    /// `key` is required: an event that cannot say which key changed would make
    /// every block refetch every publication, which is worse than missing it.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        if ev.event != Self::EVENT_NAME {
            return None;
        }
        Some(Self {
            plugin: ev.data["plugin"].as_str().unwrap_or_default().to_string(),
            key: ev.data["key"].as_str()?.to_string(),
        })
    }
}

/// `GET /api/plugins/events` — a push when a plugin's published data changes.
///
/// The counterpart to `GET /api/plugins/publications`: that answers "what is
/// true now", this says "read it again". A panel drawing a plugin's own state
/// would otherwise poll on a timer and still show a stale value between ticks.
///
/// Same shape as the file-tree stream in `routes/fs.rs`, including the
/// load-bearing ordering: subscribe the LOCAL broker channel before telling the
/// daemon to forward, because `EventBroker::dispatch` drops events for a
/// session id with no local subscriber and the window between the two calls
/// would lose the first event.
async fn publication_event_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    let rx = state.events.subscribe("system").await;
    state.daemon.subscribe_sticky("system").await.daemon_err()?;

    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .filter_map(|event| {
            // The system channel carries the file watcher and the
            // classification prompt too; this stream is only about plugin data.
            PublicationChangedEvent::from_daemon_event(&event).map(|pe| {
                let data = serde_json::to_string(&pe).unwrap_or_default();
                Ok(Event::default()
                    .event(PublicationChangedEvent::EVENT_NAME)
                    .data(data))
            })
        });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// `POST /api/plugins/command` — invoke a plugin command by name.
///
/// Not under `/{name}` because a command's name is already globally unique —
/// the daemon refuses a second plugin claiming one — so routing by plugin would
/// ask the caller for something it does not need to know. The result is passed
/// through verbatim, like publications and options: what a command returns is
/// the plugin's vocabulary, and a shape this layer validated would be a shape
/// only today's plugins could send.
async fn run_command(
    State(state): State<AppState>,
    caller: PluginCaller,
    Json(req): Json<CommandRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if req.name.trim().is_empty() {
        return Err(WebError::Validation(
            "`name` must name a command".to_string(),
        ));
    }
    refuse_another_plugins_command(&state, &caller, &req.name).await?;
    let result = state
        .daemon
        .plugin_run_command(&req.name, req.args)
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// A caller drawing for plugin X may invoke only X's commands.
///
/// The owning plugin is already on every entry `plugin.commands` returns, so
/// this is a comparison rather than a second registry — but it costs one extra
/// daemon round trip per invocation, which is why the app skips it.
///
/// A command nobody owns is refused too. An unknown name from a plugin caller
/// cannot be attributed, and "cannot attribute" is the same answer as "not
/// yours" — the alternative leaks which command names exist by their error.
async fn refuse_another_plugins_command(
    state: &AppState,
    caller: &PluginCaller,
    command: &str,
) -> Result<(), WebError> {
    let PluginCaller::Plugin(plugin) = caller else {
        return Ok(());
    };
    let commands = state.daemon.plugin_commands().await.daemon_err()?;
    let owner = commands
        .iter()
        .find(|c| c.get("name").and_then(serde_json::Value::as_str) == Some(command))
        .and_then(|c| c.get("plugin").and_then(serde_json::Value::as_str));

    match owner {
        Some(owner) if owner == plugin => Ok(()),
        _ => Err(WebError::Forbidden(format!(
            "`{command}` is not a command of plugin `{plugin}`"
        ))),
    }
}

/// `POST /api/plugins/:name/reload` — reload a plugin by name.
/// Returns the daemon's reload response (counts of tools, commands, etc.).
async fn reload_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
) -> Result<Json<serde_json::Value>, WebError> {
    caller.require_app("reload a plugin")?;
    let result = state.daemon.plugin_reload(&name).await.daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/plugins` — clone a plugin from a git URL and record it in
/// the installed manifest (`plugins.installed.json`), the same record
/// `cru plugin add` writes. The operator's own spec entries live in
/// `init.lua`, which nothing here edits. Synchronous; can take 10+ seconds.
async fn install_plugin(
    State(state): State<AppState>,
    caller: PluginCaller,
    Json(req): Json<InstallRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // The largest of the plugin routes: it clones code from a URL the caller
    // chose and loads it. Nothing a block draws needs this.
    caller.require_app("install a plugin")?;
    if req.url.trim().is_empty() {
        return Err(WebError::Validation("plugin URL must not be empty".into()));
    }
    let result = state
        .daemon
        .plugin_install(&req.url, req.branch.as_deref(), req.pin.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// `DELETE /api/plugins/:name?purge=true` — remove a plugin declaration.
async fn remove_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
    Query(query): Query<RemoveQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    caller.require_app("remove a plugin")?;
    let result = state
        .daemon
        .plugin_remove(&name, query.purge)
        .await
        .daemon_err()?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_routes_builds() {
        let _router = plugin_routes();
    }

    /// The browser listens for `publication_changed` and reads `plugin` and
    /// `key` off the frame (`web/src/components/blocks/usePublication.ts`).
    /// This is the whole contract, and it crosses a language boundary, so it
    /// gets a test on this side of it.
    #[test]
    fn a_publication_frame_keeps_the_name_and_the_fields() {
        let ev = SessionEvent::new(
            "system",
            SystemPayload::PUBLICATION_CHANGED,
            serde_json::json!({ "plugin": "kanban", "key": "kanban:board" }),
        );
        let projected = PublicationChangedEvent::from_daemon_event(&ev).expect("projects");
        assert_eq!(projected.plugin, "kanban");
        assert_eq!(projected.key, "kanban:board");
        assert_eq!(
            serde_json::to_value(&projected).unwrap(),
            serde_json::json!({ "plugin": "kanban", "key": "kanban:board" }),
            "the browser reads these two names and no others"
        );
    }

    /// The system channel also carries the file watcher and the classification
    /// prompt. A stream that forwarded those would wake every plugin block.
    #[test]
    fn another_system_event_is_not_a_publication() {
        let ev = SessionEvent::new(
            "system",
            "file_changed",
            serde_json::json!({ "path": "/w/a.md" }),
        );
        assert!(PublicationChangedEvent::from_daemon_event(&ev).is_none());
    }

    /// A frame that cannot say WHICH key changed would make every block refetch
    /// every publication, which is worse than dropping it.
    #[test]
    fn a_publication_frame_without_a_key_is_dropped() {
        let ev = SessionEvent::new(
            "system",
            SystemPayload::PUBLICATION_CHANGED,
            serde_json::json!({ "plugin": "kanban" }),
        );
        assert!(PublicationChangedEvent::from_daemon_event(&ev).is_none());
    }
}
